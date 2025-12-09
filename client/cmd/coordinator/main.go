package main

import (
	"flag"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"syscall"

	"github.com/prometheus/client_golang/prometheus/promhttp"

	"google.golang.org/grpc"
	"google.golang.org/grpc/reflection"

	"github.com/onescluster/coordinator/pkg/api"
	"github.com/onescluster/coordinator/pkg/api/middleware"
	"github.com/onescluster/coordinator/pkg/billing"
	"github.com/onescluster/coordinator/pkg/db"
	grpcMiddleware "github.com/onescluster/coordinator/pkg/middleware"
	pb "github.com/onescluster/coordinator/pkg/proto"
	"github.com/onescluster/coordinator/pkg/scheduler/gpu"
	"github.com/onescluster/coordinator/pkg/service"
	"github.com/onescluster/coordinator/pkg/supabase"
)

var (
	port = flag.Int("port", 50052, "The server port")
)

func main() {
	flag.Parse()

	lis, err := net.Listen("tcp", fmt.Sprintf(":%d", *port))
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}

	// Initialize Auth Middleware
	jwtSecret := os.Getenv("SUPABASE_JWT_SECRET")
	if jwtSecret == "" {
		log.Println("Warning: SUPABASE_JWT_SECRET not set, using mock secret")
		jwtSecret = "mock-secret"
	}

	supabaseURL := os.Getenv("SUPABASE_URL")
	supabaseKey := os.Getenv("SUPABASE_ANON_KEY")
	supabaseClient := supabase.NewClient(supabaseURL, supabaseKey)

	authInterceptor := grpcMiddleware.NewAuthInterceptor(jwtSecret)

	s := grpc.NewServer(
		grpc.UnaryInterceptor(authInterceptor.Unary()),
	)

	// Initialize Store (rqlite)
	rqliteAddr := os.Getenv("RQLITE_ADDR")
	if rqliteAddr == "" {
		rqliteAddr = "http://localhost:4001"
	}

	dbConfig := &db.Config{
		Addresses: []string{rqliteAddr},
	}

	dbClient, err := db.NewClient(dbConfig)
	if err != nil {
		log.Fatalf("Failed to connect to rqlite: %v", err)
	}

	// Initialize schema (creates tables if they don't exist)
	if err := db.InitSchema(dbClient); err != nil {
		log.Printf("Warning: failed to initialize schema: %v (continuing anyway)", err)
	}

	// Apply migrations
	if err := db.ApplyMigrations(dbClient); err != nil {
		log.Printf("Warning: failed to apply migrations: %v", err)
	}

	stateManager := db.NewStateManager(dbClient)
	// Try to initialize state from DB (load cache)
	if err := stateManager.Initialize(); err != nil {
		log.Fatalf("Failed to initialize state manager: %v", err)
	}

	// Initialize Service
	coordinatorService := service.NewCoordinatorService(stateManager)
	pb.RegisterCoordinatorServiceServer(s, coordinatorService)

	// Register reflection service on gRPC server.
	reflection.Register(s)

	// Initialize Scheduler
	scheduler := gpu.NewScheduler()

	// Initialize Handlers
	deployHandler := api.NewDeployHandler(stateManager, scheduler)
	nodeHandler := api.NewNodeHandler(stateManager)
	capsuleHandler := api.NewCapsuleHandler(stateManager, supabaseClient)

	// Initialize Billing
	stripeClient := billing.NewStripeClient()
	meteredBilling := billing.NewMeteredBillingService(stripeClient, supabaseClient)
	billingHandler := api.NewBillingHandler(stripeClient, supabaseClient)
	webhookHandler := billing.NewWebhookHandler(stripeClient, supabaseClient)
	usageHandler := api.NewUsageHandler(supabaseClient, meteredBilling)

	// Initialize HTTP Middleware
	isDev := os.Getenv("ENVIRONMENT") == "development"
	if isDev {
		log.Println("Warning: Running in DEVELOPMENT mode. Authentication is bypassed.")
	}

	jwtConfig := middleware.JWTConfig{
		Secret:  jwtSecret,
		DevMode: isDev,
	}
	jwtMiddleware := middleware.NewJWTMiddleware(jwtConfig)

	// Start HTTP server
	go func() {
		// Public endpoints (if any)
		// http.HandleFunc("/health", healthHandler.HandleHealth)

		// Metrics
		http.Handle("/metrics", promhttp.Handler())

		// Webhooks (Public)
		http.HandleFunc("/webhook/stripe", webhookHandler.HandleWebhook)

		// Protected endpoints
		http.Handle("/deploy", jwtMiddleware.Handler(http.HandlerFunc(deployHandler.HandleDeploy)))

		// Machines (Nodes)
		http.Handle("/api/v1/machines", jwtMiddleware.Handler(http.HandlerFunc(nodeHandler.HandleListNodes)))

		// Capsules
		http.Handle("/api/v1/capsules", jwtMiddleware.Handler(http.HandlerFunc(capsuleHandler.HandleListCapsules)))
		// Note: The handlers manually parse ID from path, so we map the prefix or specific path?
		// Since http.ServeMux matches patterns, "/api/v1/capsules/" matches subpaths.
		http.Handle("/api/v1/capsules/", jwtMiddleware.Handler(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if strings.HasSuffix(r.URL.Path, "/logs") {
				capsuleHandler.StreamLogs(w, r)
				return
			}
			if r.Method == http.MethodDelete {
				capsuleHandler.HandleDeleteCapsule(w, r)
			} else if r.Method == http.MethodGet {
				capsuleHandler.HandleGetCapsule(w, r)
			} else {
				http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
			}
		})))

		// Billing
		http.Handle("/api/v1/billing/checkout", jwtMiddleware.Handler(http.HandlerFunc(billingHandler.CreateCheckout)))
		http.Handle("/api/v1/billing/portal", jwtMiddleware.Handler(http.HandlerFunc(billingHandler.CreatePortalSession)))
		http.Handle("/api/v1/billing/subscription", jwtMiddleware.Handler(http.HandlerFunc(billingHandler.GetSubscription)))

		// Usage Reporting (Machine Auth TODO)
		http.HandleFunc("/api/v1/usage/report", usageHandler.HandleReportUsage)

		log.Println("HTTP server listening at :8080")
		// Use MetricsMiddleware for all requests (we might want to exclude /metrics itself if we used a mux that supported it easily,
		// but for DefaultServeMux we can just wrap the listener or individual handlers.
		// For simplicity, let's wrap the DefaultServeMux.
		if err := http.ListenAndServe(":8080", grpcMiddleware.MetricsMiddleware(http.DefaultServeMux)); err != nil {
			log.Fatalf("failed to serve HTTP: %v", err)
		}
	}()

	log.Printf("Coordinator server listening at %v", lis.Addr())

	// Graceful shutdown
	go func() {
		sigCh := make(chan os.Signal, 1)
		signal.Notify(sigCh, os.Interrupt, syscall.SIGTERM)
		<-sigCh
		log.Println("Shutting down gRPC server...")
		s.GracefulStop()
	}()

	if err := s.Serve(lis); err != nil {
		log.Fatalf("failed to serve: %v", err)
	}
}
