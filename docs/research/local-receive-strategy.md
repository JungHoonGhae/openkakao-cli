# Login-free, local-only KakaoTalk receive strategy on macOS

Status: research note (internal). Last checked on **2026-09-02**, including a
read-only schema check on macOS **26.5.2**. No KakaoTalk message content was read
for this research.

## Question

What is the most complete receive path openkakao-cli can implement without a
LOCO/REST login or server contact, and where are the unavoidable gaps?

Here, **login-free** means that openkakao-cli does not establish its own Kakao
server session. KakaoTalk itself still has to be installed, signed in, and able
to receive messages. It does not mean reception while the macOS user is logged
out.

## Headline verdict

There is no public macOS API that lets one process subscribe to another app's
delivered user notifications. Apple's API returns only **your app's** delivered
notifications that are still present in Notification Center
([`UNUserNotificationCenter`](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter),
[`getDeliveredNotifications`](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/getdeliverednotifications%28completionhandler%3A%29)).

The best feasible design is therefore a **best-effort fusion**, not a single
lossless source:

1. Notification Center DB polling as the primary, window-independent signal.
2. `AXObserver`-triggered Accessibility snapshots plus low-rate polling as the
   complementary signal for notification-suppressed/open chats.
3. A per-user `launchd` LaunchAgent for restart and login-session lifecycle.
4. A durable local inbox/dedup ledger so process restarts do not turn retained
   notification replay into duplicates.

This can materially improve the current implementation, but **100% lossless or
exactly-once receive is impossible** without a Kakao-supported receive API or a
readable authoritative message database.

## Feasibility matrix

| Candidate | Public/stable contract | Coverage and recovery | Verdict |
|---|---|---|---|
| Cross-app `UNUserNotificationCenter` | Public, but app-scoped | Cannot read or subscribe to KakaoTalk notifications | **NO-GO** |
| `usernoted` Notification Center SQLite | Path/schema/payload are private implementation details | Strongest window-independent local signal; replay only while a record remains | **Primary, best-effort** |
| AX chat-list polling | Public AX API with user-granted Accessibility trust | Visible/loaded rows; unread count and latest preview, not an event log | **Fallback/reconciliation** |
| `AXObserver` on KakaoTalk elements | Public API, but event support depends on the observed app/element | Low-latency wake-up while the target PID and AX elements exist; no replay | **Trigger, not source of truth** |
| AX message-bubble diffing | Public AX reads; KakaoTalk hierarchy is app-version-specific | Can cover already-open chats where unread stays zero; only rendered bubbles | **Tier 2 complement** |
| Distributed notifications | Public cooperative IPC | Useful only if KakaoTalk explicitly posts a known event; delivery may be delayed/dropped | **Not a receive path today** |
| Local SQLCipher message DB | Would be authoritative if readable | Current KakaoTalk builds keep the key behind an unresolved encrypted store | **Current NO-GO**; see [DB research](db-key-derivation.md) |
| `launchd` `KeepAlive` | Public process supervision | Restarts the watcher, but does not retain missed events or guarantee no restart gap | **Supervisor only** |

## 1. Cross-app notification subscription: no public API

**Sourced fact.** `UNUserNotificationCenter` is the central object for notification
behavior in “your app or app extension,” and its delivered-notification query is
explicitly limited to “your app's” notifications still visible in Notification
Center ([class documentation](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter),
[delivered-notification query](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/getdeliverednotifications%28completionhandler%3A%29)).
Its delegate processes notifications arriving for the owning app and user actions
on those notifications, not another app's stream
([`UNUserNotificationCenterDelegate`](https://developer.apple.com/documentation/usernotifications/unusernotificationcenterdelegate)).

**Conclusion.** A helper app, CLI, notification service extension, or LaunchAgent
cannot use public UserNotifications API to subscribe to KakaoTalk's delivered
notifications. Reading `usernoted` storage is not a hidden use of that API; it is
an unsupported dependency on system implementation details.

## 2. Notification Center DB: useful current state, not a queue

**Sourced fact.** Apple's only public retention statement is presence-based:
delivered notifications are returned while they are still present/visible in
Notification Center. Users can clear one notification or an entire stack
([Notification Center user guide](https://support.apple.com/guide/mac-help/get-notifications-mchl2fb1258f/mac)).
macOS also lets users disable an app's notifications, exclude them from
Notification Center, hide previews, or choose temporary versus persistent alerts;
even “persistent” means until the user dismisses it
([Notifications settings](https://support.apple.com/guide/mac-help/notifications-settings-mh40583/mac)).

**Local observation, not an API contract.** On macOS 26.5.2 the file currently at
`~/Library/Group Containers/group.com.apple.usernoted/db2/db` is a plaintext,
WAL-mode SQLite database with `app` and `record` tables matching the current
[`notif-watch` query](../../src/commands/notif_watch.rs#L344-L385). Apple publishes
no contract for this path, schema, record lifetime, or binary-plist payload.
Consequently an OS update may move it or change any of those details.

The existing choice to open with `mode=ro` and include the WAL is sound for this
unsupported path: SQLite defines `mode=ro` as read-only, permits read-only access
to WAL databases when its side files are available, and gives readers a committed
snapshot
([URI filenames](https://www.sqlite.org/uri.html),
[WAL read-only rules](https://www.sqlite.org/wal.html#readonly),
[SQLite isolation](https://www.sqlite.org/isolation.html)).

**Inference.** Treat this DB as a snapshot cache, never as durable history:

- A notification created and removed between two polls can be missed.
- A user clear, app replacement/removal, or OS cleanup before restart makes replay
  impossible.
- Notification grouping may leave several records today, but it is not a retention
  guarantee.
- Hidden previews can preserve an identifier while withholding message content.
- Schema or payload mismatch must disable this sensor cleanly, not crash-loop the
  whole receive service.

The current `--replay-existing` improves restart recovery only for records still
retained. Its in-memory tracker is bounded to the previous snapshot
([tracker](../../src/commands/notif_watch.rs#L154-L193)); therefore a supervised,
always-replay process also needs a **persistent** dedup ledger across restarts.

## 3. Accessibility observers: event hints, then read and diff

**Sourced fact.** `AXObserverCreate` creates an observer for one application PID,
and `AXObserverAddNotification` registers a particular notification on a particular
AX element
([create](https://developer.apple.com/documentation/applicationservices/1460133-axobservercreate),
[register](https://developer.apple.com/documentation/applicationservices/1462089-axobserveraddnotification)).
The observer's run-loop source must be installed before callbacks can arrive
([`AXObserverGetRunLoopSource`](https://developer.apple.com/documentation/applicationservices/1459139-axobservergetrunloopsource)).
The caller must be a trusted Accessibility client
([`AXIsProcessTrustedWithOptions`](https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrustedwithoptions)).

Apple defines useful change notifications including value, row-count, layout,
creation, destruction, and window changes
([AX notification list](https://developer.apple.com/documentation/applicationservices/carbon_accessibility/notifications),
[`kAXLayoutChangedNotification`](https://developer.apple.com/documentation/applicationservices/kaxlayoutchangednotification)).
However, registration can return `kAXErrorNotificationUnsupported`; Apple also
explains that standard AppKit controls post appropriate events while custom
controls must post their own
([registration errors](https://developer.apple.com/documentation/applicationservices/1462089-axobserveraddnotification),
[Accessibility protocol](https://developer.apple.com/documentation/appkit/nsaccessibilityprotocol)).

**Inference.** `AXObserver` can replace tight polling as a wake-up mechanism, but
cannot replace snapshot/diff logic or a watchdog poll:

- The callback says that an element changed; it is not a durable Kakao message
  event and carries no replay log.
- KakaoTalk may not post the desired event for a custom chat row or bubble.
- Virtualized tables may reuse rows, fold multiple arrivals into one unread/preview
  state, or expose only rendered children.
- App restart changes the PID; UI replacement invalidates element references and
  requires discovery and subscription again.

Register, where supported, `kAXRowCountChangedNotification`,
`kAXLayoutChangedNotification`, `kAXValueChangedNotification`,
`kAXCreatedNotification`, `kAXUIElementDestroyedNotification`, and the relevant
window-change notifications. Each callback should only mark a subtree dirty and
coalesce a near-immediate re-snapshot. Keep a slower periodic snapshot to reconcile
missed/unsupported events.

For the main chat list, diff `{chat name, unread, preview}` as today
([`ax-watch`](../../src/commands/ax_watch.rs#L20-L30)). For every already-open chat
window, diff the rendered message-bubble tail using the existing bubble parser
([AX bubble reader](../../src/ax_send.rs) — `read_visible_messages` and its
helpers). Do **not** open rooms solely
to watch them: opening changes read state and foreground behavior. Bubble identity
will be heuristic until KakaoTalk exposes a stable AX identifier, so Notification
Center `(room_id, msg_id)` identity should win whenever both sensors see the event.

## 4. `launchd`: correct supervision, not delivery durability

**Sourced fact.** Apple recommends a LaunchAgent for per-user background work; it
runs in the logged-in user's context. A LaunchDaemon runs in system context, has
no WindowServer access, and is therefore wrong for KakaoTalk AX interaction
([creating jobs](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html),
[agents versus daemons](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/DesigningDaemons.html)).

`KeepAlive=true` asks `launchd` to keep the job running, but rapidly failing jobs
are throttled; the documented default spawn throttle is ten seconds
([Apple's `launchd.plist(5)` source](https://github.com/apple-oss-distributions/launchd/blob/main/man/launchd.plist.5#L1538-L1560),
[`ThrottleInterval`](https://github.com/apple-oss-distributions/launchd/blob/main/man/launchd.plist.5#L1649-L1658)).
For a packaged macOS helper, `SMAppService` is the supported registration surface
([`SMAppService`](https://developer.apple.com/documentation/servicemanagement/smappservice)).

**Inference.** Use a per-user LaunchAgent with `RunAtLoad` and `KeepAlive`, but
assume non-zero gaps during crash loops, login/logout, sleep/wake, upgrades, and
permission failures. The process needs internal backoff, a heartbeat/health state,
and startup reconciliation. A permanent configuration or permission error should
exit distinctly or remain degraded rather than spin every ten seconds.

## 5. Distributed notifications are not Notification Center events

**Sourced fact.** `DistributedNotificationCenter` is cooperative cross-process IPC:
a process explicitly posts a named notification and registered processes receive
it. Apple warns that latency is unbounded, queues can fill and drop notifications,
and payloads are untrusted
([documentation](https://developer.apple.com/documentation/foundation/distributednotificationcenter)).
Core Foundation exposes the same kind of inter-application center and requires
registration by notification name/object
([`CFNotificationCenter`](https://developer.apple.com/documentation/corefoundation/cfnotificationcenter)).

**Conclusion.** This is separate from user-facing UserNotifications/Notification
Center. It becomes useful only if KakaoTalk deliberately posts a stable, known
distributed notification for message arrival. No such contract is known. Listening
to guessed/private names would add another lossy implementation dependency, not a
replacement for the DB or AX sensors.

## Recommended implementable architecture

### A. One long-lived receive service

Add a unified `receive-watch`/daemon core supervised by a per-user LaunchAgent.
Run all sensors in one process and normalize them into the existing
`WatchMessageEvent`; keep hooks/webhooks downstream of a local durable inbox.

### B. Primary Notification DB sensor

- Open read-only with WAL participation; never copy, mutate, checkpoint, or vacuum
  Apple's DB.
- Poll at a bounded interval (about one second is a reasonable starting point).
  A filesystem-change signal may trigger an earlier poll, but a timer remains the
  reconciliation path.
- On startup and after wake/reopen, scan retained records and consult the persistent
  ledger rather than blindly replaying all of them.
- Version/probe the expected tables, columns, bundle identifier, and payload fields;
  expose `healthy`, `permission_denied`, and `schema_incompatible` states.

### C. Complementary AX sensor

- Discover the KakaoTalk PID; attach a new observer after every launch and detach on
  termination. App lifecycle can be tracked with `NSWorkspace` launch/terminate
  notifications
  ([`NSWorkspace`](https://developer.apple.com/documentation/appkit/nsworkspace)).
- Probe each candidate AX notification and record which are supported on the
  installed KakaoTalk version.
- Use callbacks only to schedule a coalesced chat-list/open-window re-snapshot.
- Reconcile periodically even when no callback fires, and re-discover subtrees after
  layout/window/destruction events.
- Add Tier 2 bubble-tail diffing only for already-open chat windows. Keep current
  chat-list unread diffing for other visible rows.

### D. Durable fusion and delivery

Use a small openkakao-owned SQLite database, separate from Apple's DB:

- `seen_notif(room_id, msg_id, received_at)` for authoritative notification IDs.
- `seen_ax(window/chat fingerprint, bubble fingerprint, observed_at)` with a bounded
  retention period for heuristic AX events.
- `inbox(event_id, payload, source, state)` as a transactional outbox for hook/webhook
  delivery.
- Prefer a matching Notification event over AX; merge AX into it when a narrow
  chat/time/content match exists. Do not discard an ambiguous AX event merely to
  force exactly-once semantics.

Persist before invoking external hooks. This gives at-least-once sink delivery with
idempotency keys and prevents watcher crashes from losing an already-captured event.
It cannot recover an event that neither sensor observed.

### E. Health contract

Extend `doctor --json` with independent states for Notification DB readability and
schema, Kakao notifier registration, AX trust, Kakao PID/window visibility, observer
support map, LaunchAgent state, last successful sample, and durable-inbox backlog.
“Process running” must not be reported as “receive healthy” when both sensors are
degraded.

## Unavoidable blind spots

- The macOS user is logged out: a per-user LaunchAgent and GUI Accessibility session
  do not exist.
- KakaoTalk is stopped, logged out, disconnected, or itself fails to receive.
- KakaoTalk creates no user notification because a chat/app notification is muted or
  disabled, the app suppresses the currently focused room, or Focus/settings suppress
  delivery; previews may also be redacted
  ([macOS notification settings](https://support.apple.com/guide/mac-help/notifications-settings-mh40583/mac)).
- A notification appears and is removed before the DB sensor observes it, including
  during sleep, downtime, permission loss, or a launchd restart gap.
- KakaoTalk does not post a relevant AX event, replaces/virtualizes the observed
  element, or receives several messages before the next AX snapshot.
- The relevant chat row/bubbles are not present in the rendered AX tree; minimized,
  off-Space, and closed windows remain constraints of the current AX path.
- Notification DB schema/payload and KakaoTalk's AX hierarchy can change without
  notice on OS/app upgrade.
- Full Disk Access or Accessibility trust is revoked. AX trust is explicitly a user
  grant checked per process; no supervisor setting can bypass it.

## Recommended delivery order

1. Persistent notification dedup + transactional inbox, then a LaunchAgent installer
   and health/heartbeat reporting. This closes the largest restart/replay holes in the
   already-landed path.
2. `AXObserver` support probing and event-triggered chat-list reconciliation, while
   retaining the existing watchdog poll.
3. Already-open-window bubble diffing with conservative self/incoming classification
   and Notification-ID fusion.
4. A macOS/KakaoTalk compatibility fixture suite for notification payloads and AX
   tree shapes, so private-schema drift fails visibly.

Do not invest in distributed-notification guessing or claim exactly-once semantics.
Revisit the local SQLCipher DB only if a safe, versioned key-recovery path becomes
available; that is the only identified local source that could become authoritative.
