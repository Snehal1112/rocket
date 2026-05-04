package main

import (
	"fmt"
	"net/http"
)

const (
	expectedUser   = "admin"
	expectedPass   = "secret"
	expectedAPIKey = "mysecretkey"
)

func basicAuthHandler(w http.ResponseWriter, r *http.Request) {
	user, pass, ok := r.BasicAuth()
	if !ok || user != expectedUser || pass != expectedPass {
		w.Header().Set("WWW-Authenticate", `Basic realm="test"`)
		http.Error(w, "Unauthorized", http.StatusUnauthorized)
		return
	}
	fmt.Fprintf(w, "Authenticated as: %s\n", user)
}

func apiKeyHandler(w http.ResponseWriter, r *http.Request) {
	key := r.Header.Get("X-API-Key")
	if key == "" {
		key = r.URL.Query().Get("api_key")
	}
	if key != expectedAPIKey {
		http.Error(w, "Unauthorized", http.StatusUnauthorized)
		return
	}
	fmt.Fprintln(w, "Authenticated via API key")
}

func main() {
	http.HandleFunc("/basic", basicAuthHandler)
	http.HandleFunc("/apikey", apiKeyHandler)
	fmt.Println("Listening on http://localhost:8080")
	fmt.Println("  GET /basic  — Basic Auth (admin / secret)")
	fmt.Println("  GET /apikey — API Key (X-API-Key: mysecretkey) or (?api_key=mysecretkey)")
	http.ListenAndServe(":8080", nil)
}
