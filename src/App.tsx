import { useEffect, useState } from "react";
import {
  listCollections,
  createCollection,
  executeRequest,
  type CollectionSummary,
  type HttpResponse,
} from "./lib/tauri-api";
import "./App.css";

function App() {
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [newCollectionName, setNewCollectionName] = useState("");
  const [requestUrl, setRequestUrl] = useState("https://httpbin.org/get");
  const [response, setResponse] = useState<HttpResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadCollections();
  }, []);

  async function loadCollections() {
    try {
      const result = await listCollections();
      setCollections(result);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleCreateCollection(e: React.FormEvent) {
    e.preventDefault();
    if (!newCollectionName.trim()) return;
    try {
      await createCollection(newCollectionName.trim());
      setNewCollectionName("");
      await loadCollections();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleExecute(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setResponse(null);
    try {
      const result = await executeRequest({
        method: "GET",
        url: requestUrl,
        headers: [],
        auth: { authType: "none" },
        options: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
      });
      setResponse(result);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <main className="container">
      <h1>RocketAPI</h1>

      <section>
        <h2>Collections ({collections.length})</h2>
        <ul>
          {collections.map((c) => (
            <li key={c.name}>
              {c.name} — {c.requestCount} request(s)
            </li>
          ))}
        </ul>
        <form onSubmit={handleCreateCollection} className="row">
          <input
            value={newCollectionName}
            onChange={(e) => setNewCollectionName(e.currentTarget.value)}
            placeholder="New collection name"
          />
          <button type="submit">Create</button>
        </form>
      </section>

      <section>
        <h2>Execute Request</h2>
        <form onSubmit={handleExecute} className="row">
          <input
            value={requestUrl}
            onChange={(e) => setRequestUrl(e.currentTarget.value)}
            placeholder="https://..."
            style={{ width: "400px" }}
          />
          <button type="submit">Send</button>
        </form>
        {response && (
          <div>
            <p>
              Status: <strong>{response.status}</strong> {response.statusText} —{" "}
              {response.durationMs}ms — {response.sizeBytes} bytes
            </p>
            <pre style={{ maxHeight: "200px", overflow: "auto", textAlign: "left" }}>
              {response.body}
            </pre>
          </div>
        )}
        {error && <p style={{ color: "red" }}>{error}</p>}
      </section>
    </main>
  );
}

export default App;
