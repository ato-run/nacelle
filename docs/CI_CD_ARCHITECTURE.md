# CI/CD Pipeline Architecture

## Workflow Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                          GitHub Events                               │
│  • push to main/develop                                             │
│  • pull_request to main/develop                                     │
│  • tag v* (release)                                                 │
└───────────────────────────┬─────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      Parallel Build/Test Jobs                        │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐ │
│  │ build-adep-logic │  │  build-engine    │  │  build-client    │ │
│  │                  │  │                  │  │                  │ │
│  │ • Rust wasm32    │  │ • Rust native    │  │ • Go (matrix)    │ │
│  │ • Tests          │  │ • protoc         │  │   - standard     │ │
│  │ • 72KB artifact  │  │ • Test suite     │  │   - static       │ │
│  │                  │  │ • 19MB artifact  │  │ • 21MB each      │ │
│  │ ✅ contents:read │  │ ✅ contents:read │  │ • 21MB each      │ │
│  └──────────────────┘  └──────────────────┘  │ ✅ contents:read │ │
│                                               └──────────────────┘ │
│                                                                       │
│  ┌──────────────────┐                                               │
│  │  test-client     │                                               │
│  │                  │                                               │
│  │ • Go tests       │                                               │
│  │ • pkg/*          │                                               │
│  │ • e2e tests      │                                               │
│  │ ✅ contents:read │                                               │
│  └──────────────────┘                                               │
│                                                                       │
└───────────────┬───────────────────────┬─────────────────────────────┘
                │                       │
                ▼                       ▼
┌───────────────────────────┐   ┌─────────────────────────────────────┐
│  integration-test         │   │  release (if tag v*)                │
│                           │   │                                     │
│ • Build all components    │   │ • Download all artifacts            │
│ • Verify artifacts        │   │ • Create GitHub Release             │
│ • Full system check       │   │ • Attach binaries:                  │
│ ✅ contents:read          │   │   - adep_logic.wasm                 │
│                           │   │   - capsuled-engine                 │
└───────────────────────────┘   │   - capsuled-client (2 variants)    │
                                │ • Generate release notes            │
                                │ ✅ contents:write                   │
                                └─────────────────────────────────────┘
```

## Job Dependencies

```
build-adep-logic ─┐
                  ├──> integration-test ──┐
build-engine ─────┤                       ├──> release (if v* tag)
                  │                       │
build-client ─────┘                       │
                                          │
test-client ───────────────────────────────┘
```

## Trigger Matrix

| Event Type | Branch/Tag | Jobs Executed | Release Created |
|-----------|-----------|---------------|-----------------|
| push | main | All build + test + integration | ❌ No |
| push | develop | All build + test + integration | ❌ No |
| push | feature/* | ❌ None | ❌ No |
| pull_request | → main | All build + test + integration | ❌ No |
| pull_request | → develop | All build + test + integration | ❌ No |
| push tag | v* | All jobs | ✅ **Yes** |

## Artifact Flow

```
┌──────────────────────┐
│  Build Jobs          │
│  • adep-logic        │──┐
│  • engine            │  │
│  • client (×2)       │  │
└──────────────────────┘  │
                          │ upload-artifact
                          ▼
┌──────────────────────────────────────────────┐
│  GitHub Actions Artifacts                    │
│  • adep-logic-wasm/adep_logic.wasm          │
│  • capsuled-engine-linux-x86_64             │
│  • capsuled-client-linux-x86_64             │
│  • capsuled-client-linux-x86_64-static      │
│  (7-day retention)                           │
└──────────────────────────────────────────────┘
                          │
                          │ If tag v*
                          ▼
┌──────────────────────────────────────────────┐
│  GitHub Release                              │
│  • Permanent storage                         │
│  • All artifacts attached                    │
│  • Auto-generated release notes              │
└──────────────────────────────────────────────┘
```

## Build Variants

### Rust Components

```
┌─────────────────┐
│  adep-logic     │
├─────────────────┤
│ Target: wasm32  │
│ Profile: release│
│ Output: .wasm   │
│ Size: ~72KB     │
│ Use: Client+Eng │
└─────────────────┘

┌─────────────────┐
│  engine         │
├─────────────────┤
│ Target: native  │
│ Profile: release│
│ Output: binary  │
│ Size: ~19MB     │
│ Deps: protoc    │
└─────────────────┘
```

### Go Components

```
┌──────────────────────────────────────────────┐
│  client (matrix builds)                      │
├──────────────────────────────────────────────┤
│                                              │
│  ┌─────────────────────────────────────┐   │
│  │ Standard (CGO_ENABLED=1)            │   │
│  │ • Full dynamic linking              │   │
│  │ • Requires system libs              │   │
│  │ • Best for standard Linux           │   │
│  └─────────────────────────────────────┘   │
│                                              │
│  ┌─────────────────────────────────────┐   │
│  │ Static (CGO_ENABLED=0)              │   │
│  │ • Full static linking               │   │
│  │ • No external dependencies          │   │
│  │ • Portable binary                   │   │
│  │ • Compatible with Alpine/musl       │   │
│  └─────────────────────────────────────┘   │
│                                              │
│  All variants: ~21MB                        │
└──────────────────────────────────────────────┘
```

## Cache Strategy

```
┌────────────────────────────────────────┐
│  Rust Dependencies                     │
├────────────────────────────────────────┤
│ Key: cargo-{component}-{Cargo.lock}    │
│ Paths:                                 │
│  • ~/.cargo/registry                   │
│  • ~/.cargo/git                        │
│  • {component}/target                  │
│                                        │
│ Effect: ~2-3x faster builds            │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│  Go Dependencies                       │
├────────────────────────────────────────┤
│ Managed by: actions/setup-go@v5        │
│ Key: Auto (based on go.sum)            │
│ Paths:                                 │
│  • ~/go/pkg/mod                        │
│                                        │
│ Effect: ~2-3x faster builds            │
└────────────────────────────────────────┘
```

## Security Model

```
┌─────────────────────────────────────────────┐
│  GITHUB_TOKEN Permissions                   │
├─────────────────────────────────────────────┤
│                                             │
│  ┌────────────────────────────────────┐   │
│  │ Most Jobs (read-only)              │   │
│  │ • build-adep-logic                 │   │
│  │ • build-engine                     │   │
│  │ • build-client                     │   │
│  │ • test-client                      │   │
│  │ • integration-test                 │   │
│  │                                    │   │
│  │ Permissions:                       │   │
│  │   contents: read                   │   │
│  └────────────────────────────────────┘   │
│                                             │
│  ┌────────────────────────────────────┐   │
│  │ Release Job (write access)         │   │
│  │ • release                          │   │
│  │                                    │   │
│  │ Permissions:                       │   │
│  │   contents: write                  │   │
│  │                                    │   │
│  │ Condition: startsWith(ref, 'v')    │   │
│  └────────────────────────────────────┘   │
│                                             │
│  ✅ Verified by CodeQL: 0 alerts           │
└─────────────────────────────────────────────┘
```

## Timeline Estimate

```
Cold Start (no cache):
├─ build-adep-logic:   ~2-3 min
├─ build-engine:       ~5-6 min
├─ build-client (×2):  ~2-3 min each (parallel)
├─ test-client:        ~1-2 min
├─ integration-test:   ~3-4 min
└─ Total:              ~8-12 min

Warm Start (with cache):
├─ build-adep-logic:   ~30 sec
├─ build-engine:       ~2-3 min
├─ build-client (×2):  ~1 min each (parallel)
├─ test-client:        ~30 sec
├─ integration-test:   ~1-2 min
└─ Total:              ~4-6 min
```

## Usage Examples

### Trigger a Build

```bash
# Trigger on push to main
git push origin main

# Trigger on PR
gh pr create --base main --head feature/my-feature

# Create a release
git tag v1.0.0
git push origin v1.0.0
```

### Download Artifacts

```bash
# Using GitHub CLI
gh run download <run-id>

# Direct download from release
wget https://github.com/OnesCluster/capsuled/releases/download/v1.0.0/capsuled-engine
wget https://github.com/OnesCluster/capsuled/releases/download/v1.0.0/adep_logic.wasm
```

### Local Testing

```bash
# Simulate workflow locally
cd /path/to/capsuled

# Job 1: Wasm
cd adep-logic
cargo build --release --target wasm32-unknown-unknown
cargo test

# Job 2: Engine
cd ../engine
cargo build --release
cargo test

# Job 3: Client
cd ../client
go build -o capsuled-client ./cmd/client
CGO_ENABLED=0 go build -o capsuled-client-static ./cmd/client

# Job 4: Tests
go test -v ./pkg/...
```
