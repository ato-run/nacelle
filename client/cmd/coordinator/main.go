package main

import (
	"encoding/json"
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
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/reflection"

	"github.com/onescluster/coordinator/pkg/api"
	"github.com/onescluster/coordinator/pkg/api/middleware"
	"github.com/onescluster/coordinator/pkg/billing"
	"github.com/onescluster/coordinator/pkg/db"
	grpcMiddleware "github.com/onescluster/coordinator/pkg/middleware"
	"github.com/onescluster/coordinator/pkg/networking/caddy"
	"github.com/onescluster/coordinator/pkg/networking/port"
	pb "github.com/onescluster/coordinator/pkg/proto"
	coordinatorv1 "github.com/onescluster/coordinator/pkg/proto/coordinator/v1"
	"github.com/onescluster/coordinator/pkg/scheduler/gpu"
	"github.com/onescluster/coordinator/pkg/service"
	"github.com/onescluster/coordinator/pkg/store"
	"github.com/onescluster/coordinator/pkg/supabase"
)

// corsMiddleware adds CORS headers for frontend access
func corsMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Allow requests from localhost frontend
		origin := r.Header.Get("Origin")
		if origin == "" {
			origin = "*"
		}
		w.Header().Set("Access-Control-Allow-Origin", origin)
		w.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
		w.Header().Set("Access-Control-Allow-Headers", "Content-Type, Authorization, X-Requested-With")
		w.Header().Set("Access-Control-Allow-Credentials", "true")
		w.Header().Set("Access-Control-Max-Age", "86400")

		// Handle preflight
		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusOK)
			return
		}

		next.ServeHTTP(w, r)
	})
}

var (
	serverPort = flag.Int("port", 50050, "The server port")
	engineAddr = flag.String("engine", "localhost:50051", "Engine gRPC address")
)

func main() {
	flag.Parse()

	lis, err := net.Listen("tcp", fmt.Sprintf(":%d", *serverPort))
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

	// Interceptors
	// authInterceptor := grpcMiddleware.NewAuthInterceptor(jwtSecret)

	// Server options
	// opts := []grpc.ServerOption{
	// 	grpc.UnaryInterceptor(authInterceptor.Unary()),
	// 	grpc.StreamInterceptor(authInterceptor.Stream()), // Assuming StreamInterceptor would be here if used
	// }

	s := grpc.NewServer()

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
		log.Printf("Warning: Failed to connect to rqlite: %v. Running in Stateless Mode.", err)
	}

	// Initialize schema (creates tables if they don't exist)
	if dbClient != nil {
		if err := db.InitSchema(dbClient); err != nil {
			log.Printf("Warning: failed to initialize schema: %v (continuing anyway)", err)
		}

		// Apply migrations
		if err := db.ApplyMigrations(dbClient); err != nil {
			log.Printf("Warning: failed to apply migrations: %v", err)
		}
	}

	stateManager := db.NewStateManager(dbClient)
	// Try to initialize state from DB (load cache)
	if dbClient != nil {
		if err := stateManager.Initialize(); err != nil {
			log.Printf("Warning: Failed to initialize state manager: %v", err)
		}
	} else {
		log.Println("Warning: Running with nil dbClient. State persistence is disabled.")
	}

	// Initialize Networking
	caddyClient := caddy.NewClient(os.Getenv("CADDY_ADMIN_URL"))
	if err := caddyClient.EnsureBaseConfig(); err != nil {
		log.Printf("Warning: Failed to ensure Caddy base config: %v. Is Caddy running?", err)
	} else {
		log.Println("Caddy base configuration verified.")
	}

	portAllocator := port.NewAllocator()

	// Initialize SQLite Store for capsule state persistence
	dbPath := os.Getenv("GUMBALL_DB_PATH")
	if dbPath == "" {
		dbPath = "gumball.db"
	}
	capsuleStore, err := store.NewSQLiteStore(dbPath)
	if err != nil {
		log.Printf("Warning: Failed to initialize SQLite store at %s: %v. State persistence is disabled.", dbPath, err)
	} else {
		log.Printf("SQLite store initialized: %s", dbPath)
	}

	// Connect to Engine
	engineAddr := os.Getenv("ENGINE_ADDRESS")
	if engineAddr == "" {
		engineAddr = "localhost:50051" // Default for native dev
	}
	log.Printf("Connecting to Engine at: %s", engineAddr)
	engineConn, err := grpc.NewClient(engineAddr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	var engineClient pb.EngineClient
	if err != nil {
		log.Printf("Warning: did not connect to engine: %v", err)
	} else {
		defer engineConn.Close()
		engineClient = pb.NewEngineClient(engineConn)
	}

	// Register Services
	coordinatorService := service.NewCoordinatorService(dbClient, stateManager, caddyClient, portAllocator, engineClient, capsuleStore)
	coordinatorv1.RegisterCoordinatorServiceServer(s, coordinatorService)

	// Register reflection service on gRPC server.
	reflection.Register(s)

	// Initialize Scheduler
	scheduler := gpu.NewScheduler()

	// Initialize Handlers
	deployHandler := api.NewDeployHandler(stateManager, scheduler)
	nodeHandler := api.NewNodeHandler(stateManager)
	capsuleHandler := api.NewCapsuleHandler(stateManager, supabaseClient, coordinatorService)

	// Initialize Billing (Polar)
	polarToken := os.Getenv("POLAR_ACCESS_TOKEN")
	polarSandbox := os.Getenv("POLAR_SANDBOX") == "true"
	polarClient := billing.NewClient(polarToken, polarSandbox)
	meteredBilling := billing.NewMeteredBillingService(supabaseClient)
	billingHandler := api.NewBillingHandler(polarClient, supabaseClient)
	webhookSecret := os.Getenv("POLAR_WEBHOOK_SECRET")
	webhookHandler := billing.NewWebhookHandler(polarClient, supabaseClient, webhookSecret)
	usageHandler := api.NewUsageHandler(supabaseClient, meteredBilling)
	statsHandler := api.NewStatsHandler(stateManager)

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
		// Public endpoints
		http.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusOK)
			w.Write([]byte("OK"))
		})

		// Apps (Catalog) - Mock for now
		http.HandleFunc("/api/v1/apps", func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "application/json")
			w.Header().Set("Access-Control-Allow-Origin", "*")

			// Get apps from StateManager (loaded from DB)
			apps := stateManager.GetAllApps()

			json.NewEncoder(w).Encode(apps)
		})

		// Metrics
		http.Handle("/metrics", promhttp.Handler())

		// Webhooks (Public)
		http.HandleFunc("/webhook/polar", webhookHandler.HandleWebhook)
		// Temporary: keep legacy path for compatibility
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
			if strings.HasSuffix(r.URL.Path, "/stop") {
				capsuleHandler.HandleStopCapsule(w, r)
				return
			}
			if strings.HasSuffix(r.URL.Path, "/start") {
				capsuleHandler.HandleStartCapsule(w, r)
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

		// Stats (Admin Dashboard)
		http.HandleFunc("/api/v1/stats", statsHandler.HandleGetStats)

		log.Println("HTTP server listening at :8081")
		// Use MetricsMiddleware for all requests (we might want to exclude /metrics itself if we used a mux that supported it easily,
		// but for DefaultServeMux we can just wrap the listener or individual handlers.
		// For simplicity, let's wrap the DefaultServeMux.
		if err := http.ListenAndServe(":8081", corsMiddleware(grpcMiddleware.MetricsMiddleware(http.DefaultServeMux))); err != nil {
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
