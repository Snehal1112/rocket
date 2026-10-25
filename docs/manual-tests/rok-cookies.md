# Manual test: rok.cookies

Checks of `rok.cookies` and `rok.cookies.jar()` in the real app. Written 2026-10-26 from the source. None of these steps were run when this file was written.

Use a request to `https://httpbin.org`. A cookie that httpbin sets is `https://httpbin.org/cookies/set?name=value` (it redirects, so turn on the cookie jar and follow redirects). `https://httpbin.org/cookies` returns the cookies it received.

1. **Pre-request add.** Give a `GET https://httpbin.org/cookies` request this pre-request script: `await rok.cookies.add("session", "abc123");`. Send it. The body shows `session: abc123`. The cookie is in the jar.
2. **Response cookie in post-response.** Send `GET https://httpbin.org/cookies/set?token=xyz`. In the post-response script run `console.log(rok.cookies.get("token"))`. The Console shows `xyz`.
3. **Jar cross-domain read.** With the cookie from check 2 in the jar, send a request to `https://example.com` and run `console.log(await rok.cookies.jar().getCookie("https://httpbin.org", "token"))`. The Console shows the entry with `key`, `value`, `domain`, `path` and `id` set to `domain|path|name`. A name that does not exist gives `null`.
4. **Delete.** Run `await rok.cookies.delete("token")` on a httpbin request, then `console.log(rok.cookies.get("token"))`. It shows `undefined`. Send `GET https://httpbin.org/cookies` again: the cookie is gone.
5. **Clear the whole jar.** Run `await rok.cookies.jar().clear()`. There is no confirmation dialog or text; the call just resolves. Then run `console.log((await rok.cookies.jar().getCookies("https://httpbin.org")).length)` in a second script. It shows `0`, and the cookie list of the app (if open) is empty for every domain.
6. **Special characters.** Run `await rok.cookies.upsert("note", "a b=c%é")` then send `GET https://httpbin.org/cookies`. The script call must not throw. Check what httpbin echoes and that `rok.cookies.get("note")` returns the exact value you set. Then run `await rok.cookies.upsert("bad", "x; admin=1")` and `await rok.cookies.upsert("bad", "abc\n")`. Both must reject (the message never shows the value), and the next request must still send `note`.
7. **Flow transform node.** Add a Transform node to a Flow with the script `await rok.cookies.add("a", "b")`. Run the flow. The node fails with `rok.cookies is not available here`.
8. **Jar-off request.** Turn off the cookie jar option on a request. Run `await rok.cookies.add("session", "abc123");` in its pre-request script and send `GET https://httpbin.org/cookies`. The body does not show `session`. Then read it back in a script: `rok.cookies.get("session")` still returns the value, because scripts use the jar whatever the option says.
