package spec

import (
	"os"
	"testing"

	"github.com/BurntSushi/toml"
)

func TestToRunPlan_DockerHello(t *testing.T) {
	const tomlContent = `
schema_version = "1.0"
name = "hello-docker"
version = "0.1.0"
type = "app"

[execution]
runtime = "docker"
entrypoint = "ghcr.io/example/hello:latest"
port = 8080
`

	spec, err := ParseString(tomlContent)
	if err != nil {
		t.Fatalf("ParseString error: %v", err)
	}

	plan, err := ToRunPlan(spec)
	if err != nil {
		t.Fatalf("ToRunPlan error: %v", err)
	}

	if plan.CapsuleId != "hello-docker" {
		t.Fatalf("CapsuleId = %q", plan.CapsuleId)
	}
	if plan.Name != "hello-docker" {
		t.Fatalf("Name = %q", plan.Name)
	}
	if plan.Version != "0.1.0" {
		t.Fatalf("Version = %q", plan.Version)
	}

	docker := plan.GetDocker()
	if docker == nil {
		t.Fatalf("expected docker runtime")
	}
	if docker.Image != "ghcr.io/example/hello:latest" {
		t.Fatalf("docker.image = %q", docker.Image)
	}
	if len(docker.Ports) != 1 {
		t.Fatalf("docker.ports len = %d", len(docker.Ports))
	}
	if docker.Ports[0].ContainerPort != 8080 {
		t.Fatalf("docker.ports[0].container_port = %d", docker.Ports[0].ContainerPort)
	}
	if docker.Ports[0].Protocol != "tcp" {
		t.Fatalf("docker.ports[0].protocol = %q", docker.Ports[0].Protocol)
	}
}

func TestToRunPlan_DockerWithStorageVolumes(t *testing.T) {
	os.Setenv("GUMBALL_STORAGE_BASE", "/tmp/gumball-test/volumes")
	t.Cleanup(func() { _ = os.Unsetenv("GUMBALL_STORAGE_BASE") })

	const tomlContent = `
schema_version = "1.0"
name = "hello-docker-storage"
version = "0.1.0"
type = "app"

[execution]
runtime = "docker"
entrypoint = "ghcr.io/example/hello:latest"
port = 8080

[storage]
volumes = [
  { name = "data", mount_path = "/data", read_only = false },
  { name = "models", mount_path = "/models", read_only = true },
]
`

	spec, err := ParseString(tomlContent)
	if err != nil {
		t.Fatalf("ParseString error: %v", err)
	}

	plan, err := ToRunPlan(spec)
	if err != nil {
		t.Fatalf("ToRunPlan error: %v", err)
	}

	docker := plan.GetDocker()
	if docker == nil {
		t.Fatalf("expected docker runtime")
	}
	if len(docker.Mounts) != 2 {
		t.Fatalf("docker.mounts len = %d", len(docker.Mounts))
	}
	if docker.Mounts[0].Source != "/tmp/gumball-test/volumes/hello-docker-storage/data" {
		t.Fatalf("mount[0].source = %q", docker.Mounts[0].Source)
	}
	if docker.Mounts[0].Target != "/data" {
		t.Fatalf("mount[0].target = %q", docker.Mounts[0].Target)
	}
	if docker.Mounts[0].Readonly {
		t.Fatalf("mount[0].readonly = true")
	}
	if docker.Mounts[1].Source != "/tmp/gumball-test/volumes/hello-docker-storage/models" {
		t.Fatalf("mount[1].source = %q", docker.Mounts[1].Source)
	}
	if docker.Mounts[1].Target != "/models" {
		t.Fatalf("mount[1].target = %q", docker.Mounts[1].Target)
	}
	if !docker.Mounts[1].Readonly {
		t.Fatalf("mount[1].readonly = false")
	}
}

func TestParseRunPlanFile_PackageRunStyle(t *testing.T) {
	const tomlContent = `
[package]
name = "phase2-verify"
version = "0.1.0"

[run]
image = "alpine:latest"
cmd = ["/bin/sh", "-c", "echo 'Phase 2 Complete!' && sleep 5"]

[resources]
cpu = "100m"
memory = "64Mi"
gpu = "none"
`

	// Re-use the same parsing logic without touching filesystem.
	var manifest CapsuleManifestV1
	if _, err := toml.Decode(tomlContent, &manifest); err != nil {
		t.Fatalf("toml.Decode error: %v", err)
	}
	plan, err := ToRunPlanFromPackageRun(&manifest)
	if err != nil {
		t.Fatalf("ToRunPlanFromPackageRun error: %v", err)
	}

	if plan.CapsuleId != "phase2-verify" {
		t.Fatalf("CapsuleId = %q", plan.CapsuleId)
	}
	if plan.GetDocker() == nil {
		t.Fatalf("expected docker runtime")
	}
	if got := plan.GetDocker().Image; got != "alpine:latest" {
		t.Fatalf("docker.image = %q", got)
	}
	if got := plan.GetDocker().Command; len(got) != 3 {
		t.Fatalf("docker.command len = %d", len(got))
	}
	if plan.MemoryBytes == 0 {
		t.Fatalf("expected MemoryBytes to be set")
	}
}

func TestToRunPlanFromLegacy_DockerWithArgs(t *testing.T) {
	const tomlContent = `
[capsule]
name = "phase2-legacy"
version = "0.1.0"

[runtime]
type = "docker"
executable = "alpine:latest"
args = ["/bin/sh", "-c", "echo 'Phase 2 Complete!' && sleep 1"]
`

	var legacy LegacyCapsuleManifest
	if _, err := toml.Decode(tomlContent, &legacy); err != nil {
		t.Fatalf("toml.Decode error: %v", err)
	}
	plan, err := ToRunPlanFromLegacy(&legacy)
	if err != nil {
		t.Fatalf("ToRunPlanFromLegacy error: %v", err)
	}

	if plan.CapsuleId != "phase2-legacy" {
		t.Fatalf("CapsuleId = %q", plan.CapsuleId)
	}
	docker := plan.GetDocker()
	if docker == nil {
		t.Fatalf("expected docker runtime")
	}
	if docker.Image != "alpine:latest" {
		t.Fatalf("docker.image = %q", docker.Image)
	}
	if len(docker.Command) != 3 {
		t.Fatalf("docker.command len = %d", len(docker.Command))
	}
}
