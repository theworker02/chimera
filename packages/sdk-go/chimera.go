package chimera

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
)

type Client struct {
	Base string
	Auth string
	HTTP *http.Client
}

func New(base, auth string) *Client {
	return &Client{Base: base, Auth: auth, HTTP: http.DefaultClient}
}

func (c *Client) Health() (map[string]any, error) {
	req, err := http.NewRequest(http.MethodGet, c.Base+"/health", nil)
	if err != nil {
		return nil, err
	}
	if c.Auth != "" {
		req.Header.Set("Authorization", "Bearer "+c.Auth)
	}
	res, err := c.HTTP.Do(req)
	if err != nil {
		return nil, err
	}
	defer res.Body.Close()
	body, err := io.ReadAll(res.Body)
	if err != nil {
		return nil, err
	}
	if res.StatusCode >= 300 {
		return nil, fmt.Errorf("%s: %s", res.Status, string(body))
	}
	var out map[string]any
	if err := json.Unmarshal(body, &out); err != nil {
		return nil, err
	}
	return out, nil
}
