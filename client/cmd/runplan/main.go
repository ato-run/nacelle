package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"log"
	"os"

	"github.com/onescluster/coordinator/pkg/capsule"
	"google.golang.org/protobuf/encoding/protojson"
)

func main() {
	manifestPath := flag.String("f", "", "Path to capsule manifest (TOML or JSON)")
	useProto := flag.Bool("proto", false, "Output proto-compatible JSON format")
	flag.Parse()

	if *manifestPath == "" && flag.NArg() > 0 {
		*manifestPath = flag.Arg(0)
	}

	if *manifestPath == "" {
		log.Fatal("manifest path is required (-f <path>)")
	}

	manifest, err := capsule.LoadFromFile(*manifestPath)
	if err != nil {
		log.Fatalf("failed to load manifest: %v", err)
	}

	if err := manifest.Validate(); err != nil {
		log.Fatalf("manifest validation failed: %v", err)
	}

	plan, err := manifest.ToRunPlan()
	if err != nil {
		log.Fatalf("failed to convert to run plan: %v", err)
	}

	var data []byte
	if *useProto {
		// Convert to proto and marshal
		protoPlan := plan.ToProto()
		marshaler := protojson.MarshalOptions{
			Indent:          "  ",
			EmitUnpopulated: false,
		}
		data, err = marshaler.Marshal(protoPlan)
		if err != nil {
			log.Fatalf("failed to marshal proto: %v", err)
		}
	} else {
		// Standard JSON marshaling
		data, err = json.MarshalIndent(plan, "", "  ")
		if err != nil {
			log.Fatalf("failed to serialize run plan: %v", err)
		}
	}

	fmt.Fprintf(os.Stdout, "%s\n", string(data))
}
