package main

import (
	"flag"
	"log"
	"net/http"
	"os"

	"github.com/onescluster/coordinator/pkg/router"
	"github.com/onescluster/coordinator/pkg/store"
)

func main() {
	addr := flag.String("addr", ":8082", "listen address")
	dashboardURL := flag.String("dashboard", "", "dashboard upstream URL (default: http://dashboard:3000)")
	apiURL := flag.String("api", "", "api upstream URL (default: http://localhost:8080)")
	mode := flag.String("mode", "env", "routing backend: env or store")
	dbPath := flag.String("db", "", "sqlite db path for store mode (default: $GUMBALL_DB_PATH)")
	flag.Parse()

	var db router.DBClientRouterInterface
	switch *mode {
	case "store":
		path := *dbPath
		if path == "" {
			path = os.Getenv("GUMBALL_DB_PATH")
		}
		if path == "" {
			path = "gumball.db"
		}
		s, err := store.NewSQLiteStore(path)
		if err != nil {
			log.Fatalf("failed to open sqlite store: %v", err)
		}
		defer func() { _ = s.Close() }()
		db = router.NewStoreEdgeRouterDB(s, router.StoreEdgeRouterConfig{PreferLocalhost: true})
		log.Printf("edge-router using store mode: db=%s", path)
	case "env":
		envdb, err := router.NewEnvEdgeRouterDBFromEnv()
		if err != nil {
			log.Fatalf("failed to initialize edge router mappings from env: %v", err)
		}
		db = envdb
		log.Printf("edge-router using env mode")
	default:
		log.Fatalf("invalid -mode: %s (expected env|store)", *mode)
	}

	h := router.NewEdgeRouter(db, router.EdgeRouterConfig{
		DashboardURL: *dashboardURL,
		APIURL:       *apiURL,
	})

	log.Printf("edge-router listening on %s", *addr)
	log.Printf("env: GUMBALL_EDGE_ROUTE_MAP lines: user/capsule=url")
	log.Printf("env: GUMBALL_EDGE_CUSTOM_DOMAIN_MAP lines: domain=user/capsule")

	if err := http.ListenAndServe(*addr, h); err != nil {
		log.Fatalf("listen failed: %v", err)
	}
}
