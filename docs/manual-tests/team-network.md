# Manual test: team network

Checks of discovery, pairing and presence on real machines. Written 2026-11-06 from the plan. None of these steps were run when this file was written, so the first pass is also the check of the steps.

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
