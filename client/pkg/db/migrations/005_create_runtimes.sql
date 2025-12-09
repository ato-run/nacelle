CREATE TABLE IF NOT EXISTS runtimes (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  type TEXT NOT NULL CHECK(type IN ('native', 'docker', 'wasm')),
  description TEXT,
  latest_version TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS runtime_versions (
  runtime_id TEXT NOT NULL,
  version TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  download_url TEXT NOT NULL,
  PRIMARY KEY (runtime_id, version),
  FOREIGN KEY (runtime_id) REFERENCES runtimes(id)
);

INSERT INTO runtimes (id, name, type, latest_version) VALUES
  ('llama-server', 'gumball-llama-server', 'native', 'v1.0.0'),
  ('flux-webui', 'flux-webui', 'docker', 'latest');
