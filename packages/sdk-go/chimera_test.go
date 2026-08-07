package chimera

import "testing"

func TestNewClient(t *testing.T) {
	c := New("http://127.0.0.1:7600", "admin:ops")
	if c.Base == "" {
		t.Fatal("empty base")
	}
}
