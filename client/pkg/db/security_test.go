package db

import (
	"testing"
	"time"
)

// TestSQLInjectionPrevention tests that SQL injection attempts are properly escaped
func TestSQLInjectionPrevention(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected string
	}{
		{
			name:     "single quote injection",
			input:    "test'; DROP TABLE nodes; --",
			expected: "test''; DROP TABLE nodes; --",
		},
		{
			name:     "multiple single quotes",
			input:    "it's a test's value",
			expected: "it''s a test''s value",
		},
		{
			name:     "normal string",
			input:    "normal-string-123",
			expected: "normal-string-123",
		},
		{
			name:     "empty string",
			input:    "",
			expected: "",
		},
		{
			name:     "UNION injection attempt",
			input:    "' UNION SELECT * FROM nodes--",
			expected: "'' UNION SELECT * FROM nodes--",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := escapeSQLString(tt.input)
			if result != tt.expected {
				t.Errorf("escapeSQLString(%q) = %q, expected %q", tt.input, result, tt.expected)
			}
		})
	}
}

// TestNodeCreationWithMaliciousInput tests that malicious input doesn't break node creation
func TestNodeCreationWithMaliciousInput(t *testing.T) {
	maliciousInputs := []string{
		"'; DROP TABLE nodes; --",
		"' OR '1'='1",
		"admin'--",
		"' UNION SELECT password FROM users--",
		"1'; UPDATE nodes SET is_master=1--",
	}

	for _, malicious := range maliciousInputs {
		t.Run("malicious_input_"+malicious, func(t *testing.T) {
			node := &Node{
				ID:            malicious,
				Address:       malicious,
				HeadscaleName: malicious,
				Status:        NodeStatusActive,
				IsMaster:      false,
				LastSeen:      time.Now(),
				CreatedAt:     time.Now(),
				UpdatedAt:     time.Now(),
			}

			// This should not panic and should properly escape the input
			// We're testing the query construction, not actual execution
			escaped := escapeSQLString(node.ID)
			if escaped == malicious {
				t.Errorf("Input was not escaped: %q", malicious)
			}

			// Verify that single quotes are properly doubled
			if containsSingleQuote(malicious) && !containsDoubledQuote(escaped) {
				t.Errorf("Single quotes not properly escaped in: %q -> %q", malicious, escaped)
			}
		})
	}
}

// TestCapsuleCreationWithMaliciousManifest tests manifest injection prevention
func TestCapsuleCreationWithMaliciousManifest(t *testing.T) {
	maliciousManifests := []string{
		`{"name": "test'; DROP TABLE capsules; --"}`,
		`'; DELETE FROM capsules WHERE '1'='1`,
		`' OR 1=1--`,
	}

	for _, manifest := range maliciousManifests {
		t.Run("malicious_manifest", func(t *testing.T) {
			capsule := &Capsule{
				ID:            "test-capsule",
				Name:          "test",
				NodeID:        "test-node",
				Manifest:      manifest,
				Status:        CapsuleStatusPending,
				StoragePath:   "/storage/test",
				BundlePath:    "/bundles/test",
				NetworkConfig: `{"ip": "192.168.1.1"}`,
				CreatedAt:     time.Now(),
				UpdatedAt:     time.Now(),
			}

			escaped := escapeSQLString(capsule.Manifest)
			if escaped == manifest && containsSingleQuote(manifest) {
				t.Errorf("Manifest was not escaped: %q", manifest)
			}
		})
	}
}

// TestMetadataInjectionPrevention tests metadata key/value injection
func TestMetadataInjectionPrevention(t *testing.T) {
	tests := []struct {
		name  string
		key   string
		value string
	}{
		{
			name:  "malicious key",
			key:   "key'; DROP TABLE cluster_metadata; --",
			value: "safe-value",
		},
		{
			name:  "malicious value",
			key:   "safe-key",
			value: "'; DELETE FROM cluster_metadata; --",
		},
		{
			name:  "both malicious",
			key:   "' OR '1'='1",
			value: "' OR '1'='1",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			escapedKey := escapeSQLString(tt.key)
			escapedValue := escapeSQLString(tt.value)

			if escapedKey == tt.key && containsSingleQuote(tt.key) {
				t.Errorf("Key was not escaped: %q", tt.key)
			}

			if escapedValue == tt.value && containsSingleQuote(tt.value) {
				t.Errorf("Value was not escaped: %q", tt.value)
			}
		})
	}
}

// TestResourceAllocationInjection tests resource allocation ID injection
func TestResourceAllocationInjection(t *testing.T) {
	maliciousIDs := []string{
		"node'; DROP TABLE node_resources; --",
		"capsule' OR '1'='1",
		"'; UPDATE node_resources SET cpu_allocated=0--",
	}

	for _, id := range maliciousIDs {
		t.Run("malicious_id_"+id, func(t *testing.T) {
			escaped := escapeSQLString(id)
			if escaped == id {
				t.Errorf("ID was not escaped: %q", id)
			}

			// Ensure no SQL keywords pass through unescaped
			if containsSQLKeywords(id) && escaped == id {
				t.Errorf("SQL keywords in ID were not escaped: %q", id)
			}
		})
	}
}

// Helper functions

func containsSingleQuote(s string) bool {
	for _, c := range s {
		if c == '\'' {
			return true
		}
	}
	return false
}

func containsDoubledQuote(s string) bool {
	// Check if string contains ''
	for i := 0; i < len(s)-1; i++ {
		if s[i] == '\'' && s[i+1] == '\'' {
			return true
		}
	}
	return false
}

func containsSQLKeywords(s string) bool {
	keywords := []string{"DROP", "DELETE", "UPDATE", "INSERT", "SELECT", "UNION", "OR", "AND", "--"}
	for _, keyword := range keywords {
		if len(s) >= len(keyword) {
			// Simple case-insensitive check
			for i := 0; i <= len(s)-len(keyword); i++ {
				match := true
				for j := 0; j < len(keyword); j++ {
					c1 := s[i+j]
					c2 := keyword[j]
					// Simple uppercase comparison
					if c1 >= 'a' && c1 <= 'z' {
						c1 = c1 - 'a' + 'A'
					}
					if c1 != c2 {
						match = false
						break
					}
				}
				if match {
					return true
				}
			}
		}
	}
	return false
}

// TestEscapeSQLStringEdgeCases tests edge cases in SQL escaping
func TestEscapeSQLStringEdgeCases(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected string
	}{
		{
			name:     "empty string",
			input:    "",
			expected: "",
		},
		{
			name:     "only single quote",
			input:    "'",
			expected: "''",
		},
		{
			name:     "multiple consecutive quotes",
			input:    "'''",
			expected: "''''''",
		},
		{
			name:     "unicode characters",
			input:    "test'日本語'test",
			expected: "test''日本語''test",
		},
		{
			name:     "newlines and tabs",
			input:    "test'\nwith\tnewline",
			expected: "test''\nwith\tnewline",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := escapeSQLString(tt.input)
			if result != tt.expected {
				t.Errorf("escapeSQLString(%q) = %q, expected %q", tt.input, result, tt.expected)
			}
		})
	}
}
