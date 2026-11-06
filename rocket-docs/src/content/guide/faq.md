# FAQ & troubleshooting

## Does Rocket need an account or an internet connection?

No. There's no cloud and no sign-in — Rocket works fully offline, and every
collection, environment and request stays on your machine.

## Where is my data stored?

Locally, at `~/.rocket-api/<workspace-name>/`, with each collection under
`collections/<collection-name>/` inside that. Collections themselves can also
live wherever you put them on disk if you're using an external (non-embedded)
collection — see [Collections & requests](/guide/collections-and-requests).

## Where does the AI Assistant store its conversations?

Per workspace, under `assistant/conversations/`, deliberately kept out of git
(a `.gitignore` covers the whole folder) since conversations can reference
request content even though secret values in them are masked before they're
saved. See [AI Assistant](/guide/ai-assistant).

## Is Rocket open source?

Yes — MIT licensed, and the whole thing is readable on
[GitHub](https://github.com/Snehal1112/rocket) before you install it.

## How does Rocket compare to Postman or Bruno?

Rocket uses the OS's own WebView (WebKitGTK/WKWebView/WebView2) instead of
bundling Chromium, and a Rust backend with no garbage collector or Node
runtime — so it uses roughly half the idle memory of either Electron-based
client and takes noticeably less disk space installed. The honest trade-off:
**startup is Rocket's weak spot** — the UI is slower to draw on first launch
than either Bruno or Postman. The full measured numbers and methodology are
published in the
[benchmarks doc](https://github.com/Snehal1112/rocket/blob/main/docs/engineering/benchmarks.md).

## Can I migrate away from Rocket later?

Yes — collections export to a Bruno ZIP or a Postman Collection v2.1 file, see
[Import & export](/guide/import-and-export). Migrating away is meant to be as
frictionless as migrating in.

## Where do I report a bug or ask a question?

Open an issue on [GitHub](https://github.com/Snehal1112/rocket/issues), or
email <snehal.dangroshiya@numericlabs.io> directly.
