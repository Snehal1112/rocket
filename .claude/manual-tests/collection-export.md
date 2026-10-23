# Manual test: collection export

Checks of the Bruno ZIP and Postman exports against the real tools. Written 2026-10-23 from the source. None of these steps were run when this file was written, so the first pass is also the check of the commands.

Contents

- Part 0: set up a sample collection
- Part 1: export in the app
- Part 2: schema validation with ajv
- Part 3: run the Postman file with newman
- Part 4: Bruno converter and `bru run`
- Part 5: open the Bruno ZIP in Bruno 3
- Part 6: import into Postman
- Part 7: secret leak grep
- Part 8: environments and scripts

## Part 0: set up a sample collection

Make a collection named `Export Demo` with:

- A folder `users` with a folder header `X-Team: blue` and a post-response script `rok.setEnvVar('id', res.getBody().id);`.
- A request `Get user` (GET `https://httpbin.org/anything/:id`, path param `id=1`, one disabled query param `debug=1`) with an assertion `res.status eq 200` and a tests script `test('ok', () => { expect(res.getStatus()).to.equal(200); });`.
- A request `Login` (POST, JSON body) with Bearer auth and the literal token `literal-token-123`, and a header `Authorization: Bearer literal-header-456`.
- A request with a script that has an alias: `const g = rok.getVar; g('a');`.
- A request with the unmapped call `rok.sendRequest({})`.
- An environment `prod` with a plain variable `HOST=httpbin.org` and a secret variable `API_KEY=literal-secret-789`. Add a second environment `staging` that extends `prod`.

## Part 1: export in the app

1. Right click the collection and choose `Export...`.
2. Choose `Bruno collection (ZIP)` and keep `Include environments` on. Save as `/tmp/export-demo-bruno.zip`.
3. Choose `Postman collection (v2.1)`, keep `Include environments` on and `Preserve scripts` off. Save as `/tmp/export-demo-postman.zip`.
4. Export Postman again with `Include environments` off. Save as `/tmp/export-demo.postman_collection.json`.
5. Expected: the dialog shows a report. It lists the blanked token, the blanked header, the blanked secret variable, the skipped `rok.sendRequest` call and the preserved alias script. The description says that bodies, scripts and descriptions are not scanned.

## Part 2: schema validation with ajv

```bash
mkdir -p /tmp/export-check && cd /tmp/export-check
unzip -o /tmp/export-demo-postman.zip
curl -sL https://schema.getpostman.com/json/collection/v2.1.0/collection.json -o schema.json
npx ajv-cli@5 validate --spec=draft7 -s schema.json -d export-demo.postman_collection.json
```

Expected: `valid`. If ajv rejects the schema itself, add `-c ajv-formats` and note the error here.

## Part 3: run the Postman file with newman

```bash
npx newman run export-demo.postman_collection.json -e environments/prod.postman_environment.json
```

Expected: `Get user` runs, its assertion passes, and the folder header `X-Team: blue` is on the request.

## Part 4: Bruno converter and `bru run`

```bash
npx --package=@usebruno/converters --package=@usebruno/cli -- node -e "
const { postmanToBruno } = require('@usebruno/converters');
const fs = require('fs');
const out = postmanToBruno(JSON.parse(fs.readFileSync('export-demo.postman_collection.json', 'utf8')));
fs.writeFileSync('bruno-converted.json', JSON.stringify(out, null, 2));
"
```

Expected: the file is written without an error and holds the same requests. Then import `bruno-converted.json` in Bruno and run `Get user`.

## Part 5: open the Bruno ZIP in Bruno 3

1. In Bruno 3 choose Open Collection, then the ZIP `/tmp/export-demo-bruno.zip`.
2. Expected: one collection `Export Demo` with folders and requests, environments `prod` and `staging`, and no `workspace.yml`, `.git` or `node_modules`.
3. Scripts use `rok.*` and need `bru.*` calls to run in Bruno. That is expected and reported.

## Part 6: import into Postman

1. In Postman choose Import and pick `/tmp/export-demo-postman.zip`'s extracted collection JSON and both environment files.
2. Expected: folders, requests, disabled params shown unchecked, the `prod` environment with `API_KEY` as a secret type with an empty current value.
3. Open the `Get user` test tab. Expected: the `pm.test` lines and the assertion line.

## Part 7: secret leak grep

```bash
cd /tmp/export-check && rm -rf bruno && mkdir bruno && cd bruno && unzip -o /tmp/export-demo-bruno.zip
cd /tmp/export-check
grep -rE "literal-token-123|literal-header-456|literal-secret-789" . && echo LEAK || echo clean
```

Expected: `clean`. Any hit is a bug.

## Part 8: environments and scripts

- `staging` must hold the variables of `prod` and its own overrides in the Postman ZIP.
- `API_KEY` must have `"type": "secret"` and `"value": ""`.
- With `Preserve scripts` on, the Postman scripts must still read `rok.*` while assertions are still translated.
- Export a collection with no environments and `Include environments` on. Expected: a plain `.postman_collection.json`, not an empty ZIP.
