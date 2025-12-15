-- Create apps table
CREATE TABLE IF NOT EXISTS apps (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    image TEXT NOT NULL,
    version TEXT NOT NULL,
    category TEXT,
    icon_url TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Seed initial data
INSERT OR IGNORE INTO apps (id, name, description, image, version, category, icon_url) VALUES 
('nginx-demo', 'Nginx Demo', 'Simple web server for testing', 'nginx', 'alpine', 'Web Server', 'https://raw.githubusercontent.com/docker-library/docs/master/nginx/logo.png'),
('whoami', 'Whoami', 'HTTP server that shows container info', 'containous/whoami', 'latest', 'Utility', 'https://raw.githubusercontent.com/containous/whoami/master/whoami.png'),
('httpbin', 'HTTPBin', 'HTTP Request & Response Service', 'kennethreitz/httpbin', 'latest', 'Utility', 'https://upload.wikimedia.org/wikipedia/commons/thumb/e/ec/Heroku_logo.svg/2560px-Heroku_logo.svg.png');
