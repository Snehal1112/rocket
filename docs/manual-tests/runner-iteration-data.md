# Manual test: runner iteration data

Checks of the Runner tab data file, the saved default and `rok.runner.iterationData` in the real app. Written 2026-11-12 from the source. None of these steps were run when this file was written. Feature doc: `docs/features/runner-iteration-data.md`.

Setup: a collection `api` with two requests, `A` and `B`, to `https://httpbin.org/anything`. Make the data files in a folder outside the collection unless a check says otherwise.

1. **CSV, three rows.** Save `users.csv` with the lines `name,age`, `ann,30`, `bob,41`, `cy,52`. Open the Runner tab of `api`, click "Choose data file" and pick it. The row says `3 rows (name, age)`. Click Run. Three sections appear, one per row, each with `A` and `B`. Nothing is sent after row three.
2. **JSON.** Save `users.json` with `[{"name":"ann","age":30},{"name":"bob","active":true,"note":null}]`. Pick it and run. Two passes run. A script sees `rok.runner.iterationData.get('age')` as `"30"` (a string) in row one, and `get('active')` as `"true"` and `get('note')` as an empty string in row two. `has('age')` is false in row two.
3. **Wrong extension.** Pick a `.txt` file. The row shows an error that the file must end in `.csv` or `.json`. A run does not start and nothing is sent.
4. **Empty file.** Pick an empty `.csv`. The row shows an error (no rows, or the header cannot be read). No request is sent.
5. **Header only.** A `.csv` with only `name,age`. The error says the file has no rows.
6. **1001 rows.** Make a `.csv` with a header and 1001 rows. The error says it has more than 1000 rows. With exactly 1000 rows it loads.
7. **Duplicate header.** A `.csv` with the header `name,name`. The error names the duplicate header.
8. **Long CSV row.** A `.csv` with the header `a,b` and a row `1,2,3`. The error says CSV row 1 has more cells than the header. A row `1` (fewer cells) loads and `b` is an empty string.
9. **Big file.** A `.csv` over 10 MB. The error says the file is too large.
10. **Saved default.** In the Overview tab of `api`, type `data/users.csv` in "Runner data file" and click Save in the Runner card. Put the file at `<collection>/data/users.csv`. The Runner tab shows `Using saved default: data/users.csv`. Run: one pass per row.
11. **Per-run file wins, then back.** In the same tab pick another file. The row shows that file. Click the X ("Use saved default"). The saved default shows again.
12. **Second machine.** Commit the collection and pull it on a second machine, or edit `opencollection.yml` by hand to add `runner: { dataFile: data/users.csv }` under `extensions.rocketapi`. Open the collection. The Overview field and the Runner tab show the default. Other keys under `extensions` (for example `bruno.scripts.flow`) are still in the file after a save.
13. **Invalid saved default.** Set the saved default to each of `/etc/passwd`, `../users.csv` and `data\users.csv`. In the Overview tab a warning says it does not look like a relative path, and Save still works. In the Runner tab the row shows "The saved default data file cannot be read" and a run does not start. Then make `<collection>/out.csv` a symlink to a file outside the collection and save `out.csv`. The Runner tab shows the error that the file must stay inside the collection folder. Clear the field and save. The Runner tab says "No data file, the run goes once." and a run sends each request once.
14. **Overview field edits.** Type `  data/users.csv  ` with spaces and save. Reopen the tab: the value has no spaces. Clear the field and save. The key is gone from `opencollection.yml`.
15. **Another card does not delete it.** With a saved default set, change the Docs card only and save. Look at `opencollection.yml`: the data file key is still there.
16. **Stop during pass two.** With a 3-row file and a slow request (`https://httpbin.org/delay/3`), click Stop while pass two runs. No new request starts. The open pass and pass three show their remaining requests as skipped. Run again: it starts from row one.
17. **Script reads the row.** Give request `A` a pre-request script `console.log('pre', rok.runner.iterationData.get('name'), rok.runner.iterationIndex, rok.runner.totalIterations)`, a post-response script that logs the same with `post`, and a tests script that logs `rok.runner.iterationData.stringify()`. Run the 3-row file. The Console shows `pre ann 0 3` and `post ann 0 3`, then `bob 1 3`, then `cy 2 3`, and the tests script logs `{"age":"30","name":"ann"}` for row one (keys are alphabetical). `get()` with no key returns the whole row and `has('nope')` is false.
18. **No data file.** Clear every data file and run a request that logs the same members. `get('name')` is `undefined`, `iterationIndex` is `0` and `totalIterations` is `1`.
19. **Row is read-only.** In a script run `rok.runner.iterationData.get().name = 'x'` and read `get('name')` again: it is unchanged. `rok.runner.iterationData.unset` is `undefined`.
20. **Nested `rok.runRequest`.** Give `A` the pre-request script `await rok.runRequest('B')` and give `B` a pre-request script that logs `rok.runner.iterationData.get('name')`. Run the 3-row file. The nested `B` logs the same name as its caller in each pass.
21. **1000 rows.** Run a 1000-row file against a fast local server. The window stays responsive: you can scroll the results and click Stop while it runs, and Stop ends the run.
