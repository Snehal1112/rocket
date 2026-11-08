# Manual test: team network

Checks of discovery, pairing, presence and sending a collection on real machines. Written 2026-11-06 from the plan. None of these steps were run when this file was written, so the first pass is also the check of the steps.

Contents

- Setup
- 1: two machines on one Wi-Fi
- 2: pairing
- 3: restart and reconnect
- 4: sleep and wake
- 5: network change
- 6: Add by address over VPN or Tailscale
- 7: declined firewall prompt
- 8: Wi-Fi with client isolation
- 9: hub with 30 teammates
- 10: hide, block, remove
- 11: send a collection

## Setup

- Two machines, A and B, with the same Rocket build, on the same Wi-Fi. A third machine C helps in checks 9 and 10.
- Start each with `bun run tauri dev` or an installed build.
- To start clean on a machine: quit Rocket, delete `~/.rocket-api/network/` and the keychain entry with service `com.rocketapi.device-identity`. The machine then gets a new device id.
- Each machine shows its own short device id in Settings, Team network.

## 1: two machines on one Wi-Fi

1. Start A, then B. Do not touch either.
2. Wait at most 5 seconds. Open the radar icon in the status bar on A.
3. Expect B under Nearby with its name, initials and short device id, and a Pair button. B shows A the same way.
4. Type the first letters of B's name, then B's initials, then B's short id (with and without the dash). Each finds B. Type `zzz`: the panel says no nodes match.
5. First launch only: a card says "You're visible on this network as '<name>'." Press OK. Restart: the card does not return.

## 2: pairing

1. On A press Pair next to B. A shows a dialog with a 6-digit code (like `482 913`), B's name and B's short id.
2. B shows a toast "<A> wants to pair" and, in the panel, a request at the top. Press Review.
3. Expect the same code on both screens.
4. Press Accept on A, then on B. Both show "Paired with <name>." and the other node moves to Paired with the chip Online.
5. Repeat the pairing and press Decline on B. A shows "<B> declined." and nothing is saved.
6. Repeat and leave the request unanswered for 2 minutes. Expect "The request expired." and the dialog on B switches to the same text if it was open.
7. Press Pair on A again right after a decline. Expect the request to be refused for about 1 minute (busy).

## 3: restart and reconnect

1. With A and B paired and online, quit Rocket on B and start it again.
2. Expect B to show A as Paired and Online within about 30 seconds, with no action. A shows B going Offline, then Online again.
3. Quit both and start both. Expect the same result.

## 4: sleep and wake

1. Put B to sleep for 2 minutes. A shows B Offline after 40 to 60 seconds (two missed pings: A goes Offline when it sends the third ping, pings are 20 seconds apart).
2. Wake B. Expect B Online on A again within about 2 minutes, without any click.

## 5: network change

1. Move B from the Wi-Fi to another network (or switch Wi-Fi off and on). Move it back.
2. Expect B Offline while away and Online again after it returns. The pairing is still there.
3. Join both to a different Wi-Fi. Expect them to find each other again with the same pairing.
4. On a third machine C that is nearby but not paired, switch the Wi-Fi off (no goodbye is sent). Expect C to leave A's Nearby list within about 2 minutes (the mDNS record expiry).

## 6: Add by address over VPN or Tailscale

1. Put A and B on different networks joined by a VPN or Tailscale.
2. On A open the panel and press Add by address. Enter B's VPN address or Tailscale name and the port from B's Settings (default 47821).
3. Expect B under Nearby on A within about a minute. Try port `0` and `70000`: the dialog refuses them. Try a closed port: the dialog or the list shows no node, with no crash.
4. Pair as in check 2. Restart both: the pair reconnects through the saved address.
5. In Settings, Team network, remove the saved address. Expect the pair to stay paired.

## 7: declined firewall prompt

1. On a machine with a firewall (macOS or Windows), start Rocket for the first time and deny the incoming connection prompt.
2. Expect the banner "Others can't connect to you. Allow Rocket in your firewall settings." in the panel.
3. Outgoing still works: pairing needs only this machine to reach the other one, so pressing Pair on a node this machine can see works, even though others cannot connect to it first.
4. Allow Rocket in the firewall settings, restart, and expect the banner to be gone.

## 8: Wi-Fi with client isolation

1. Join A and B to a Wi-Fi with client isolation (guest Wi-Fi).
2. Expect "No Rocket nodes found on this network." and the hint "VPNs and some Wi-Fi networks block discovery." Add by address still shows its dialog.
3. If mDNS is blocked on the machine, expect "Automatic discovery isn't available here." and no hint.

## 9: hub with 30 teammates

Needs 30 nodes. Use real teammates or 30 machines or VMs.

1. Pick one node as the hub H. Each teammate presses Pair on H, one at a time or a few at once.
2. On H review each request from the panel (several open requests list at the top, each with its own toast). Accept each one. After 5 requests in a minute extra requests see "busy"; the teammate tries again a minute later.
3. When all 30 are paired, expect 30 rows under Paired, all Online, and the status bar number 30. Searching by one name finds one row. The panel stays smooth to scroll.
4. Leave it for 10 minutes. Expect all 30 to stay Online (the connection limit is 256).
5. The in-process version of this check is in the Rocket infra tests (one node paired with 30 nodes).

## 10: hide, block, remove

1. On B turn on Hide my node in Settings, Team network. A stops seeing B under Nearby within about 2 minutes, but B stays Online under Paired on A.
2. On C, try to pair with B: C does not see B. Turn Hide off: C sees B again.
3. On B press Block in a request from C. C sees "<B> declined." B lists C under Blocked devices. Press Unblock: C can request again.
4. On A remove B from the Paired menu and confirm. B sees "<A> removed the pairing." and A moves to Nearby (or disappears). Repeat with B offline: B sees the message when it reconnects, and the pairing is gone on both.
5. Change the display name on A (Settings). B shows the new name after the next connection. A name of 41 characters is refused.
6. Open Settings, Team network on a machine where the keychain is locked or unavailable. Expect networking off with the reason, in the panel and in Settings.

## 11: send a collection

Needs two paired machines, A and B, ideally one Linux and one Mac, both Online. Before each check, make sure B has an active workspace you can inspect. Collection names below are examples.

1. Push, Linux to Mac. On A (Linux) create a collection `Orders` with a folder, 3 requests and one collection variable. Right-click it, pick **Send to...**, pick B, leave **Include environments** off, press Send. A toast on A says "Sent to <B>. Waiting for them to accept." B shows a toast "<A> wants to send you a collection" with the name, the size and A's short id. Press Review, check the same details in the dialog, press Accept. B shows "Receiving from <A>" and then the toast `Received "Orders" from <A>.`. Expect a new collection `Orders` in B's active workspace with the same folder, requests and variable. A shows `Sent "Orders" to <B>.`. Both Audit logs have one row ("Sent collection Orders to device ..." on A, "Received collection Orders from device ..." on B, no outcome in brackets).
2. Push, Mac to Linux. Repeat check 1 with the roles swapped.
3. Menu item. Take B offline (quit Rocket on B). Open the collection menu on A, both the `...` menu and the right-click menu. Expect no **Send to...** item. Start B again and wait for Online: the item is back in both menus.
4. Decline. Send `Orders` from A. On B press Decline. Expect "<B> declined." on A, no toast on B about its own choice, no new collection on B, and no sender audit row on A (nothing was sent).
5. Expiry. Send from A. On B open the dialog and leave it for 2 minutes. Expect the dialog to switch to "The request expired." with a Done button and no Accept, and "The request expired." on A. Repeat but leave the toast alone (do not open the dialog): the toast goes away and A shows the same message. No collection on B and no audit row on either side.
6. Close without answering. Send from A, press Review on B, close the dialog with Escape. Expect the toast to come back. Press Review again and Accept. The collection arrives.
7. Name clash. B already has a collection `Orders`. Send `Orders` from A and accept. Expect a new collection `Orders (from <A>)` on B and the old `Orders` untouched. Send it again: expect `Orders (from <A>)-1` (the importer suffix).
8. Secret and script. In `Orders` on A add a bearer token auth with a real-looking value, a variable named `api_token` with a value, and a pre-request script that logs a distinct line with `console.log`. Send with **Include environments** on, with an environment that also holds an `api_token` value. On B, before Accept, expect the dialog's warning that the scripts run in the Safe sandbox and that Developer mode, host environment and vault access stay off. Accept on B. Expect the received files (open the collection folder on B) to contain neither the token nor the `api_token` value. Open a request on B and send it: the script runs in the Safe sandbox on this first Send (the log line shows). Expect Developer mode to be off for the collection and a script that needs it (host env or vault) to be denied until you allow it in the collection's trust settings.
9. Over 10 MiB. In a collection add request bodies until the export is over 10 MiB (a few large JSON bodies). Press Send. Expect the dialog to stay open with an inline error "This collection is too big to send (X MiB, limit 10)." and no toast on B. Remove content until the ZIP is just under 10 MiB and expect the send to work.
10. Wi-Fi drops mid-transfer. Use a collection of about 8 MiB. Accept on B, and as soon as B shows the receiving line switch off Wi-Fi on B (or A). Expect "Sending failed. Try again." on A and "Receiving failed." on B, no new collection, and no leftover file: check that `~/.rocket-api/` has no staged transfer file. A has one Audit row ending in "(failed)" because chunks were sent. Switch Wi-Fi back on and wait for both nodes to be Online, then send again: it works.
11. Two offers at once. Send two different collections from A in a row. Expect two toasts on B, each opening its own dialog with its own name and size. Accept one and decline the other.
12. One send per peer. Send `Orders` from A to B and leave the offer unanswered. Send another collection from A to B. Expect A's send dialog to show the inline error "A transfer to that node is already running." and no second toast on B. (The "<B> is busy. Try again in a minute." case needs four senders; it is covered by the automated test `the_fourth_pending_offer_is_told_busy` in Plan 4.)
13. Environments with a secret variable (the keychain path). On A create an environment for `Orders` with a variable marked secret that holds a value, and a plain variable. Send with **Include environments** on and accept on B. Expect the environment on B with the plain value kept and the secret variable present but blank, and no OS keychain prompt or error on B during the import.
14. Odd names. Send a collection named `My "Quoted" API` and one with a name that has spaces and an accented letter. Expect the names to show unchanged in the toast, the dialog and the result toasts, and the received collection to have a usable name.
15. Cancel. Send from A and, before B answers, press **Cancel** on A's "Waiting for them to accept." toast. Expect the toast to go away, B's toast and dialog to close, and "<A> cancelled the transfer." on B only, with no sender audit row. Then send a large collection (about 8 MiB), accept on B and watch A's toast change to "Sending to <B>...". Press Cancel on A while chunks flow: it cancels. Send it again and this time press **Cancel** in B's receiving dialog: expect "<B> cancelled the transfer." on A and no toast about it on B. Try once more and wait for the end: once the last chunk is out, A's toast has no Cancel action any more, and a normal result follows. Known open item: the toast action closes the toast even when the cancel is refused. Press Cancel right as the last chunk goes out and note whether the toast closes while the transfer still finishes.
16. Offline during the offer. Send from A, open the dialog on B, then quit Rocket on A. Expect B's dialog to switch to "The other node went offline." with a Done button (no Accept), and the same text in a toast.
17. Notes. In `Orders` on A add a flow (or a gRPC request) and a bearer token. Send. Expect A's dialog to stay open with a list "Not sent with the collection" that names what was skipped or blanked, and a Done button. The waiting toast shows at the same time.
18. Import fails on B. Make the import fail on B (for example fill the disk quota of the workspace folder, or make the workspace folder read-only). Send from A and accept. Expect B's toast to show "The collection could not be imported." (a fixed text, not the importer's error), A to show "Sending failed. Try again.", and no half-written collection on B.
19. Sent means imported. Send a collection and watch A: the "Sent" toast must appear only after B finished the import, not when the last chunk leaves A.
20. Unconfirmed. Send an about 8 MiB collection, accept on B, and quit Rocket on B a few seconds after A's toast lost its Cancel action (last chunk out, so B has long since acknowledged it and is importing). Expect "Sent, but the other node did not confirm." on A as an info toast, and an Audit row on A ending in "(unconfirmed)". This is hard to reproduce, because the import is fast. Use a slower import, or kill the receiver right when Cancel disappears. If the import finishes first, "Sent to B." with a completed row is also an acceptable result.
21. Staging purge with two instances (known open item). Start a receive of an about 8 MiB collection on B, and while it runs start a second Rocket instance on B. The startup purge of the second instance is not lock-aware, so it may delete the first instance's staged file. Expect the receive on the first instance either to finish or to fail with "Receiving failed." and note which. Do not rely on this combination.
22. Finished before the send command returns. Hard to trigger by hand: send to a node and press Decline as fast as possible. Expect exactly one "<B> declined." toast on A and no stuck "Waiting for them to accept." toast. The automated store tests cover the ordering.
