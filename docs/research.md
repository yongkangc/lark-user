# Reference source research

Inspected on 2026-10-07 after cloning all three repositories into `/tmp`. Pinned
source evidence is distinct from current live Feishu or international Lark proof.

| Reference | Commit | License |
| --- | --- | --- |
| ryanwx/lark-bridge | `a165f59cc053216a40fe17fc6854924b29f8fc61` | [MIT, Ryan Zhu](https://github.com/ryanwx/lark-bridge/blob/a165f59cc053216a40fe17fc6854924b29f8fc61/LICENSE) |
| lza6/feishu-message-2API | `b3a1684608294b4ba8cf6ad4e42b8455e7d77f2e` | [Apache-2.0](https://github.com/lza6/feishu-message-2API/blob/b3a1684608294b4ba8cf6ad4e42b8455e7d77f2e/LICENSE) |
| EthanQC/feishu-user-plugin | `171e92fdfb35e0371dfb81546e9c1e63e7fc1cda` | [MIT, EthanQC](https://github.com/EthanQC/feishu-user-plugin/blob/171e92fdfb35e0371dfb81546e9c1e63e7fc1cda/LICENSE) |

Rust code and deterministic fixtures in this project are original. Subsequent
ports or vendored schemas must retain upstream copyright/license notices;
Apache-derived changes also require applicable attribution/change notices.

## lark-bridge

Confirmed in source: the UI domain config affects Origin/Referer, while internal
API, Drive, frontier and login remain fixed Feishu hosts:
[_urls.py:8–23](https://github.com/ryanwx/lark-bridge/blob/a165f59cc053216a40fe17fc6854924b29f8fc61/src/lark_bridge/_urls.py#L8-L23).
The name "Lark" and configurable domain do not prove international support.

Cookie-only message-ID search uses command `11021`, manually encoded fields and
an opaque pagination token; body fetch uses command `8`:
[search.py:110–253](https://github.com/ryanwx/lark-bridge/blob/a165f59cc053216a40fe17fc6854924b29f8fc61/src/lark_bridge/search.py#L110-L253).
It requires a known chat ID, so it does not establish complete conversation
discovery. This implementation is chat/time/filter oriented; general keyword
search on international Lark is unproven. Non-200/decode failures can become empty
results. Fixed ID-length assumptions and disabled TLS verification must not be
carried into the new client.

Cookie/protobuf sending is present in
[sender.py](https://github.com/ryanwx/lark-bridge/blob/a165f59cc053216a40fe17fc6854924b29f8fc61/src/lark_bridge/sender.py).
WebSocket setup additionally depends on `passport_web_did`, a web app key, derived
access key and a frontier ticket, with push `6` / ACK `1`:
[listener.py:88–198](https://github.com/ryanwx/lark-bridge/blob/a165f59cc053216a40fe17fc6854924b29f8fc61/src/lark_bridge/listener.py#L88-L198).
Reconnect code does not prove missed-message recovery or exactly-once delivery.
These are Feishu source-supported paths; no live request/send was performed.

## feishu-message-2API

The provider fixes its backend to Feishu and requires captured cookie, browser
user-agent, Referer, command ID and web version; CSRF headers are optional inputs:
[feishu_provider.py:18–51](https://github.com/lza6/feishu-message-2API/blob/b3a1684608294b4ba8cf6ad4e42b8455e7d77f2e/app/feishu_provider.py#L18-L51).
History uses Frame/BizRequest/GetMessages:
[provider:54–108](https://github.com/lza6/feishu-message-2API/blob/b3a1684608294b4ba8cf6ad4e42b8455e7d77f2e/app/feishu_provider.py#L54-L108),
[schema:6–77](https://github.com/lza6/feishu-message-2API/blob/b3a1684608294b4ba8cf6ad4e42b8455e7d77f2e/app/feishu_im.proto#L6-L77).
Its envelope differs from lark-bridge's Packet/Frame layouts. Neither is selected
without comparing actual international requests and decoded responses.

The collector guesses history versus stream command from body presence, with
sorted-command fallback heuristics:
[get_cookie.py:50–79](https://github.com/lza6/feishu-message-2API/blob/b3a1684608294b4ba8cf6ad4e42b8455e7d77f2e/get_cookie.py#L50-L79),
[get_cookie.py:138–146](https://github.com/lza6/feishu-message-2API/blob/b3a1684608294b4ba8cf6ad4e42b8455e7d77f2e/get_cookie.py#L138-L146).
The provider streams repeated HTTP POST responses; this is not proof of an
international authenticated WebSocket. Command classification remains a hypothesis.
The collector prints credential values; that behavior is unsuitable for an
agent transcript and is not reproduced here.

## feishu-user-plugin

Cookie initialization bootstraps CSRF, reads user info and captures
`swp_csrf_token` / `sl_session` with fixed Feishu API/Origin/Referer:
[user.js:6–146](https://github.com/EthanQC/feishu-user-plugin/blob/171e92fdfb35e0371dfb81546e9c1e63e7fc1cda/src/clients/user.js#L6-L146).
Built-in web-client app/version/header values are not evidence that international
Lark uses the same constants; they are distinct from user-created app credentials.

The cookie class implements text/post sending (`5`), contact/group search
(`11021`), P2P creation (`13`) and group details (`64`):
[user.js:161–231](https://github.com/EthanQC/feishu-user-plugin/blob/171e92fdfb35e0371dfb81546e9c1e63e7fc1cda/src/clients/user.js#L161-L231),
[user.js:354–421](https://github.com/EthanQC/feishu-user-plugin/blob/171e92fdfb35e0371dfb81546e9c1e63e7fc1cda/src/clients/user.js#L354-L421).
P2P creation is a mutation and cannot be a read-only discovery fallback.

Message history, user chat listing and message keyword search route through
official/UAT clients:
[im-read.js:207–312](https://github.com/EthanQC/feishu-user-plugin/blob/171e92fdfb35e0371dfb81546e9c1e63e7fc1cda/src/tools/im-read.js#L207-L312).
The README narrows its app-free promise:
[README.en.md:16–18](https://github.com/EthanQC/feishu-user-plugin/blob/171e92fdfb35e0371dfb81546e9c1e63e7fc1cda/README.en.md#L16-L18).
Its stated international realtime limit is also explicit:
[README.en.md:275–285](https://github.com/EthanQC/feishu-user-plugin/blob/171e92fdfb35e0371dfb81546e9c1e63e7fc1cda/README.en.md#L275-L285).
Cookie contact search cannot be equated with OAuth message search, and advertised
feature lists cannot be treated as cookie-only capabilities wholesale.

## International conclusion

Confirmed: inspected implementations assume Feishu hosts and offer useful
session/protocol research. Unknown: this enterprise's tenant/region, session
requirements, catalogs, read/search/thread schemas, frontier and quotas. No
regional substitution, guessed endpoint/header or borrowed wire schema is active.
The missing evidence is an authorized international session and read capture for
one explicitly approved enterprise chat.
