# TeamSpeak 3/6 wire protocol — engineering reference extracted from ts3j

**Source of truth:** `vendor/ts3j` (Maven `com.github.manevolent:ts3j:1.0.3`), single squashed commit
`db57d60c989e399626aa16d921390f5033e6cdeb` ("Merge pull request #54 from BeckMario/master",
2026-01-05), working tree clean, Apache-2.0. The README states it is "an open-source implementation of
the reverse-engineered Teamspeak3 full server/client protocol, as an adaptation of Splamy's C# TS3Client"
(`README.md:3`).

**Citation convention.** Everything below cites `file:line`.
- `X/Y.java:12` = `vendor/ts3j/src/main/java/com/github/manevolent/ts3j/X/Y.java` line 12.
- `test/Foo.java:12` = `vendor/ts3j/src/test/java/com/github/manevolent/ts3j/Foo.java`.
- `ex/…` = `vendor/ts3j/examples/audio/src/main/java/com/github/manevolent/ts3j/examples/audio/…`.
- Evidence marked **[outside ts3j]** comes from the workspace TS6 server testbed, not from the repo.
- Anything the repo does not implement is written **NOT IMPLEMENTED / not found** — never inferred.

Code comments in the source repeatedly anchor to
`https://github.com/ReSpeak/tsdeclarations/blob/master/ts3protocol.md`
(e.g. `protocol/header/PacketHeader.java:82`, `protocol/header/ClientPacketHeader.java:27`), i.e. the
authors treated that document as the spec while writing these structs.

---

## 0. Verdict up front

| Area | Status in ts3j |
|---|---|
| UDP voice transport, framing, ACK/retransmit, fragmentation | **Implemented and exercised** against real servers |
| `initivexpand2` (license-based) crypto | **Implemented** with known-answer tests |
| `initivexpand` (legacy) crypto | **Implemented** with known-answer tests |
| Command escaping/parsing, notify→event mapping | **Implemented** |
| Voice packet format | **Implemented** (transport only; codec is *not* in the library) |
| Opus encode/decode | **Only in `examples/audio`**, not in the library |
| File transfer (TCP 30033) | **NOT IMPLEMENTED / not found** — only response DTOs |
| Server-version negotiation / `version_sign` generation | **NOT IMPLEMENTED / not found** — hard-coded constants |
| MTU negotiation, latency mode, PTT/VAD | **NOT IMPLEMENTED / not found** |
| TS6 interop | **Plausible but unproven**: the crypto handshake is the TS3 ≥3.4 shape TS6 still accepts; the blockers are the fake client version/sign and the missing subsystems, not the crypto |

---

## 1. Transport

### 1.1 Socket/protocol/ports
- **UDP only** for the game/voice/command protocol. `socket = new DatagramSocket();`
  (`protocol/socket/client/LocalTeamspeakClientSocket.java:371`), send/receive via
  `DatagramPacket` (`:203`, `:267`); the whole abstract layer is "used to interact with the UDP
  connection" (`protocol/socket/client/AbstractTeamspeakClientSocket.java:40`).
- The local socket is **not bound to 9987** — it is an ephemeral OS-assigned port; the README even
  documents `port // UDP client port, Teamspeak3 client uses 9987` as the *remote* port
  (`README.md:54`).
- Port constants (`protocol/TS3DNS.java:13-16`): voice `9987`, server-query `10011`, filetransfer
  `30033`, tsdns `41144`. Only **9987** is actually used for the connection; the other three are
  declared and never used by any code path.
- Host resolution: SRV record `_ts3._udp.<domain>` via dnsjava (`protocol/TS3DNS.java:18-19,40-42`),
  sorted by priority then weight (`:30-35`); on `UnknownHostException` the client falls back to
  `new InetSocketAddress(hostname, 9987)` (`protocol/socket/client/LocalTeamspeakClientSocket.java:334-337`).
- Inbound packets are filtered by **source IP + port** equality with the remote
  (`LocalTeamspeakClientSocket.java:216-219`) — anti-spoofing at the socket layer.

### 1.2 Packet framing
Two header shapes, both little-/big-endian **BIG_ENDIAN** (`LocalTeamspeakClientSocket.java:231`,
`AbstractTeamspeakClientSocket.java:431`), MAC first:

```
client -> server   (13 byte header)
  0..7   MAC(8)          AES-EAX tag, or "TS3INIT1", or fake MAC
  8..9   PId  uint16 BE  sequence number  (field name: packetId)
 10..11  CId  uint16 BE  client id        (only present client->server)
 12      PT   uint8      (type & 0x0F) | (flags & 0xF0)
 then body

server -> client   (11 byte header)
  0..7   MAC(8)
  8..9   PId  uint16 BE  sequence number
 10      PT   uint8      (type & 0x0F) | (flags & 0xF0)
 then body
```
Derivation: `PacketHeader.writeHeader` writes MAC then `packetId`
(`protocol/header/PacketHeader.java:81-87`, size 8+2 at `:97-99`); `ClientPacketHeader` appends
`clientId` then the type/flags byte (`protocol/header/ClientPacketHeader.java:24-35`, size +3 at
`:55-58`); `ServerPacketHeader` appends only the type/flags byte
(`protocol/header/ServerPacketHeader.java:13-24`, size +1 at `:39-42`).
Round-trip proof on a real captured client→server voice packet (13-byte header, `CId=2`):
`test/VoicePacketTest.java:24-32`.

The `MAC` field position is the *first 8 bytes of the datagram*, i.e. the encrypted layout is
`MAC(8) || header-without-MAC || ciphertext` (`protocol/packet/transformation/PacketTransformation.java:105-117`).

### 1.3 Packet type ids and per-type behaviour
`protocol/packet/PacketBodyType.java:6-14` (index, acked-by, canEncrypt, canResend, splittable,
compressible, mustEncrypt):

| id | type | kind | acked by | can resend | splittable | compressible | must encrypt |
|---|---|---|---|---|---|---|---|
| 0x0 | VOICE | SPEECH | – | no | no | no | no |
| 0x1 | VOICE_WHISPER | SPEECH | – | no | no | no | no |
| 0x2 | COMMAND | CONTROL | ACK | yes | yes | yes | **yes** |
| 0x3 | COMMAND_LOW | CONTROL | ACK_LOW | yes | yes | yes | **yes** |
| 0x4 | PING | KEEPALIVE | PONG | no | no | no | no |
| 0x5 | PONG | KEEPALIVE | – | no | no | no | no |
| 0x6 | ACK | CONTROL | – | yes | no | no | no |
| 0x7 | ACK_LOW | CONTROL | – | yes | no | no | no |
| 0x8 | INIT1 | CONTROL | – | no | no | no | no |

Header flags (`protocol/header/HeaderFlag.java:3-9`), high nibble of PT:
`0x10 FRAGMENTED`, `0x20 NEW_PROTOCOL`, `0x40 COMPRESSED`, `0x80 UNENCRYPTED`.

Per-type body formats (all big-endian, no length prefix — the datagram length delimits):
- ACK / ACK_LOW: `uint16 ackedPacketId` (`protocol/packet/PacketBody6Ack.java:28-41`,
  `PacketBody7AckLow.java:28-41`).
- PING: empty body, forces `UNENCRYPTED` (`PacketBody4Ping.java:14-32`).
- PONG: `uint16 echoedPacketId`, forces `UNENCRYPTED` (`PacketBody5Pong.java:23-41`).
- COMMAND / COMMAND_LOW: raw UTF-8 command text, no terminator
  (`PacketBody2Command.java:39-52`, `PacketBody3CommandLow.java:39-52`).
- INIT1: `4-byte version || uint8 step || step payload` (`protocol/packet/PacketBody8Init1.java:54-63`,
  `:103-106`); the 4-byte version is only written by the client (`:57-58`) and is
  `0C FF D2 FE` (`protocol/packet/handler/client/LocalClientHandlerConnecting.java:28`).
- VOICE / VOICE_WHISPER: see §5.

### 1.4 MTU / fragmentation
- Hard maximum UDP payload is **500 bytes**, both directions:
  `Fragments.MAXIMUM_PACKET_SIZE = 500` (`protocol/packet/fragment/Fragments.java:21`) and the receive
  buffer is a fixed `new byte[500]` (`LocalTeamspeakClientSocket.java:203`) — **any inbound datagram
  larger than 500 bytes is silently truncated by the JDK**, there is no reassembly of oversized
  datagrams.
- Outbound split (`Fragments.split`, `:28-106`):
  1. if `packet.getSize() > 500` and the type is compressible, the body is QuickLZ-compressed
     (`QuickLZ.compress(buf,1)`, `:39`) and the `COMPRESSED` flag is set (`:43`); if it now fits, it is
     sent whole (`:53-54`);
  2. otherwise the byte stream is cut into `500 - headerSize` pieces (`:63-67`);
  3. **only the first and the last fragment carry `FRAGMENTED`** (`:85`); non-first fragments have all
     flags cleared (`:75-76`); `NEW_PROTOCOL` is propagated to every fragment (`:79-82`);
  4. all fragments are written while holding a per-type lock so packet ids stay contiguous
     (`AbstractTeamspeakClientSocket.java:348-351` — the comment at `:344-347` states exactly this).
- Non-splittable types (voice, whisper, ping, pong, init1, acks) throw if too large
  (`AbstractTeamspeakClientSocket.java:353-356`). Voice payload budget is therefore 500−13 = 487 bytes.
- Inbound reassembly (`protocol/packet/fragment/PacketReassembly.java:24-114`): a per-type queue keyed
  by packet id; it walks ids from the last-assembled counter and stops at the first `FRAGMENTED`
  *even* transition (first/last marker semantics, `:53-64`); incomplete chains return `null`
  (`:69`); the reassembled first-fragment header (cloned) is reused (`:81-87`); decompression with
  a 1 MiB cap (`:95`).
- **No MTU negotiation is sent by this client** — `clientinit` contains no MTU-ish parameter
  (`LocalClientHandlerConnecting.java:231-280`). 500 is a compile-time constant.

### 1.5 Sequence numbers and generations
- One 16-bit counter **per packet type** on each side: `LocalCounterFull(65536, true)` and
  `RemoteCounterFull(65536, 100)` (`AbstractTeamspeakClientSocket.java:118-129`); INIT1 gets the
  zero counters (`:120-122`, `LocalCounterZero`/`RemoteCounterZero` at `:1089-1116`, `:1365-1395`).
- The wire carries only the 16-bit id; the **generation (wrap count) is never transmitted** — each
  side derives it from the receive window and uses it as AES key material (§2.4). Receive side:
  `counter.getGeneration(packetId)` then `header.setGeneration(...)`
  (`AbstractTeamspeakClientSocket.java:643-649`; window logic `RemoteCounterFull.getGeneration`
  `:1153-1191`, dedupe `put` `:1203-1277`, 100-slot history window).
- Voice bodies reuse the header id: `((PacketBody0Voice)body).setPacketId(header.getPacketId())`
  (`:407-411`) — so voice "sequence" == the VOICE-type counter, 16-bit, wrapping.
- `RemoteCounterFullModulated` exists (`:1304-1341`) but is **never instantiated** (dead code;
  `getCurrentGeneration` throws `UnsupportedOperationException` at `:1339`).

### 1.6 ACK / retransmit / RTT
- Every received type with `acknolwedgedBy != null` triggers an immediate ACK/ACK_LOW/PONG
  (`AbstractTeamspeakClientSocket.java:663-696`); PING is answered with a PONG carrying the same id and
  the packet is *not* passed upward (`:677-694`).
- Outbound resendable types (COMMAND, COMMAND_LOW, and ACK/ACK_LOW bodies themselves) are tracked in
  `sendQueue`/`sendQueueLow`/`pingQueue` with `maxTries = canResend() ? 30 : 0`
  (`:444-475`), then blocking-waited **1000 ms per attempt** (`:490-514`), resending the *same already
  encrypted datagram* (`PacketResponse.resend`, `:992-1005`). Exhausting 30 tries throws
  `TimeoutException("send")` (`:512-513`). Worst case ≈ 30 s per command.
- Duplicate suppression: a resendable packet is dropped unless `counter.put(id)` says it is new
  (`:763-769`).
- RTT: RFC6298-style smoothing `alpha=0.125, beta=0.25`, `rto = srtt + max(0, 4·rttvar)`
  (`:88-92`, `:736-757`); last 5 PONG samples kept for `getPing()` (`:741-748`, `:798-822`).
- `currentRto` is only *displayed*/acknowledgement-timeout-adjacent; there is **no RTO-driven adaptive
  timer** — the 1000 ms fixed wait is what actually governs resends (`:498`).

### 1.7 Timeouts and keepalive
- `AbstractTeamspeakClientSocket.TIMEOUT = 30000L` (`:47`). While `CONNECTED`, socket read timeout is
  `min(1000, 1000 - sinceLastPing)` (`:622-624`); on timeout: if `now - lastResponse >= 30000`
  → `setState(DISCONNECTED)` (`:631-633`), else send PING and record `lastPing` (`:635-636`).
  Net effect: **PING at ~1 Hz when idle, hard drop after 30 s of silence.**
- Connect timeout is caller-supplied: `waitForState(CONNECTED, timeout)`
  (`LocalTeamspeakClientSocket.java:396`); README example uses 10000 ms (`README.md:57`).
- INIT1 step 127 = "retry": the client sleeps 1000 ms and restarts from step 0
  (`LocalClientHandlerConnecting.java:127-134`).
- Disconnect: `clientdisconnect [reasonid=8 reasonmsg=…]` then `waitForState(DISCONNECTED, 30000)`
  (`LocalTeamspeakClientSocket.java:1270-1303`).

### 1.8 Connection/client id
- Client id starts at 0 (`setClientId(0)`, `LocalTeamspeakClientSocket.java:360`) and is written into
  every outbound header (`AbstractTeamspeakClientSocket.java:301-302`).
- The server assigns the real id in its `initserver` reply: `aclid` → `setClientId`, `acn` → nickname,
  `virtualserver_id` → serverId (`LocalTeamspeakClientSocket.java:772-787`). Any packet sent before
  that carries `CId = 0`.

---

## 2. Crypto and identity

### 2.1 Two-phase crypto
1. **Bootstrap ("fake") crypto** — a fixed AES-EAX key/nonce used until the server hands out key
   material: key `"c:\windows\system"`, nonce `"m\firewall32.cpl"` (both 16 ASCII bytes), fake MAC =
   8 zero bytes (`protocol/packet/transformation/InitPacketTransformation.java:8-16`).
   It is literally plaintext with a constant; **it provides no confidentiality or authentication**
   (`computeParameters` returns the constants verbatim, `:18-21`).
2. **Session crypto** — AES-128-EAX with per-packet key/nonce derived from an `ivStruct` obtained from
   `initivexpand` (ECDH) or `initivexpand2` (license-derived ECDH) (§3.4).

### 2.2 Cipher
- `new EAXBlockCipher(new AESEngine())` — **AES in EAX mode (AEAD), 8-byte (64-bit) tag**
  (`protocol/packet/transformation/PacketTransformation.java:27`, `:79-84` `new AEADParameters(key,
  8*MAC_LEN=64, nonce, associatedText)`), `MAC_LEN = 8` (`:19`). BouncyCastle `bcprov-jdk15on:1.67`
  (`pom.xml:37-41`).
- Associated data = the header **without** the MAC (`:76-84`, `:126-135`) — i.e. `PId‖CId?‖PT` is
  authenticated but not encrypted.
- Key/nonce derivation (`:35-63`):
  ```
  buf  = [0x30 if role==SERVER else 0x31] ‖ uint8(typeId) ‖ int32_BE(generation) ‖ ivStruct
  h    = SHA-256(buf)
  key  = h[0..16)          (16 bytes)
  nonce= h[16..32)         (16 bytes)
  key[0] ^= (packetId >> 8) & 0xFF
  key[1] ^= (packetId >> 0) & 0xFF
  ```
  Note the **role byte 0x30/0x31 is `'0'`/`'1'` ASCII**, and that `generation` and `packetId` are inputs
  to the key — so a wrong generation window or a reused id breaks decryption (this is the classic
  TS3 generation desync failure, surfaced as `"failed to decrypt …"` at
  `AbstractTeamspeakClientSocket.java:557-567`).
- Encrypt: body is the plaintext, output `MAC(8)‖header-without-MAC‖ciphertext`; the EAX tag is
  stored in the header's MAC slot (`PacketTransformation.java:89-119`). The 8-byte tag on the wire is
  the EAX tag XORed with the CTR keystream, because decryption feeds the stored 8 bytes *after* the
  ciphertext through `processBytes` before `doFinal` (`:140-150`) — a byte-exact reimplementation must
  reproduce that quirk.
- **No stream cipher, no RSA on the data path, no HMAC** anywhere in the repo (grep: only
  `EAXBlockCipher`/`AESEngine`/`GCM`-free). `hash128` is a **misnamed SHA-1** helper
  (`util/Ts3Crypt.java:35-51`), `hash256` = SHA-256 (`:53-59`), `hash512` = SHA-512 (`:61-67`).

### 2.3 Identity (client keypair) and UID
- Identity is **ECDSA on P-256 / `prime256v1`**, not Ed25519: `LocalIdentity.generateNew` builds an
  `ECKeyPairGenerator` over `ECNamedCurveTable.getParameterSpec("prime256v1")`
  (`identity/LocalIdentity.java:77-94`); signing is ECDSA-with-SHA256
  (`util/Ts3Crypt.java:208-226`, `DSADigestSigner(new ECDSASigner(), new SHA256Digest())` at `:219`).
- Public key encoding = DER `SEQUENCE { BIT STRING(0), INTEGER 32, INTEGER x, INTEGER y }`
  (`identity/Identity.java:40-49`; base64 of it is `getPublicKeyString()`, `:36`).
- **`uid = Base64( SHA1( ASCII( Base64(DER(pubkey) ) ) ) )`** — `Identity.generateUid()`
  (`identity/Identity.java:149-153`) hashes `getPublicKeyString()` as ASCII and wraps it in `Uid`
  (base64 of the 20 digest bytes, `identity/Uid.java:15-18`). Known-answer:
  private key → pubkey string `MEsDAgcAAgEgAiBpPRbTliVt…` and uid `27QHSPRzQe+enxh56eUokkLmsFg=`
  (`test/ClientUidGenerationTest.java:25-28`).
- **Security level / key offset**: brute-forces a decimal offset appended to the base64 pubkey string
  such that `SHA1(pubkeyString ‖ decimal(offset))` has `level` leading zero bits
  (`identity/Identity.java:79-147`, `getLeadingZeroBits` `:134-147`). `client_key_offset` is that
  offset (`LocalClientHandlerConnecting.java:271-273`). Known-answer: offset 2294, level 10
  (`test/ClientUidGenerationTest.java:19-23`).
- Identity file format (TS3 `.ini`): `"<offset>V<base64>"` where the base64 body is
  `Base64(DER-with-private)` XOR-obfuscated with a **fixed ASCII key** of 100+ bytes and then XORed
  with its own SHA-1 prefix (`identity/LocalIdentity.java:36-39`, `:184-241` for export, `:257-314`
  for read; private-key DER variant `DERBitString 128` at `:108-119`). Read/export are
  round-trip tested (`test/IdentitySaveTest.java`, `test/IdentityFileTest.java:18-37`).
- Ed25519 **does** appear, but only for the ephemeral `clientek` key (§3.4) and the license chain
  (§2.5): `generateKeypair` clamps a 32-byte scalar and does `ge_scalarmult_base`
  (`util/Ts3Crypt.java:272-288`), backed by the vendored `Punisher.NaCl` ref10 port
  (`src/main/java/Punisher/NaCl/**`).

### 2.4 `initivexpand` path (legacy)
`Ts3Crypt.cryptoInit(alpha, beta, omega, identity)` (`util/Ts3Crypt.java:69-94`):
```
sharedKey = SHA1( X( omega_point · identityPrivate ) )      // P-256 ECDH, X padded/truncated to 32 B
ivStruct  = ( sharedKey[0..10) XOR alpha(10) ) ‖ ( sharedKey[10..10+len(beta)) XOR beta )
fakeSig   = SHA1(ivStruct)[0..8)
```
- `beta` must be 10 or 54 bytes, else `IOException` (`:72-73`); `alpha` is fixed 10 bytes.
- `getSharedSecret` decodes `omega` as a **DER P-256 point** and multiplies by
  `identity.getPrivateKey()` (`:96-113`); X is left-padded/right-truncated to 32 bytes before SHA-1
  (`:100-110`). The same `ivStruct`/`fakeSig` construction is duplicated in
  `getSecureParameters` (`:162-177`).
- **No signature check of `omega`/`alpha` in this path** — the legacy exchange is unauthenticated.
  (Contrast `initivexpand2` which verifies a proof: `LocalClientHandlerConnecting.java:177-180`.)
- Known-answer test: `test/CryptoInitTest.java:37-49` pins shared secret, ivStruct and fake signature.

### 2.5 `initivexpand2` path (license / accounting)
`Ts3Crypt.cryptoInit2(license, alpha, beta, privateKey)` (`util/Ts3Crypt.java:115-127`):
1. `License.readLicenses(l)` — blob starts with version byte `0x01` (`license/License.java:157-162`);
   each block is `uint8 keyType(=0x00) ‖ pubkey[32] ‖ uint8 blockType ‖ int32 start ‖ int32 end ‖
   use-body` (`:89-99`); `blockType` ∈ {0 INTERMEDIATE, 2 SERVER, 8 TS5SERVER, 32 EPHEMERAL}
   (`license/LicenseUseType.java:4-7`); SERVER use body = `uint8 type ‖ 4 unknown ‖ utf8z issuer`
   (`license/ServerLicenseUse.java:26-44`).
2. Per-block hash = **SHA-512 of the block minus its first byte, truncated to 32 bytes**
   (`license/License.java:174-181`).
3. Chain derivation from a hard-coded root public key
   (`License.ROOT_KEY`, `license/License.java:12-18`):
   `key = ROOT_KEY; for each block: key = ed25519_add( scalarmult(clamp(blockHash), blockPubKey), key )`
   (`deriveKey`, `:192-233`), with `final[31] ^= 0x80` (`:221`).
4. `sharedSecret = SHA-512( ge_scalarmult_vartime( privateKey[31]&=0x7F , negate(key) ) ‖ ^0x80 )`
   (`Ts3Crypt.generateSharedSecret2`, `util/Ts3Crypt.java:179-201`).
5. `ivStruct`/`fakeSig` exactly as in §2.4 (`getSecureParameters`, `:162-177`).
- Known-answer test with a real license blob, alpha, beta and private key:
  `test/CryptoInit2Test.java:20-34` — `ivStruct = E4082A92…F4513`, `fakeSig = 54F2B4D661E0F9AB`.
- Root key derivation is separately pinned: `test/LicenseDerivationTest.java:42-47`.
- **`tvd` (license validation ticket) is parsed-out then ignored — the code is commented out**
  (`LocalClientHandlerConnecting.java:163-165`). No client-side accounting/reporting exists anywhere in
  the repo (grep for `account…` yields nothing in `src/main`).

### 2.6 Obfuscated / hard-coded material (complete list)
| Item | Value | Site |
|---|---|---|
| init cipher key | `c:\windows\system` | `InitPacketTransformation.java:8-9` |
| init cipher nonce | `m\firewall32.cpl` | `InitPacketTransformation.java:11-12` |
| INIT1 MAC magic | `TS3INIT1` | `AbstractTeamspeakClientSocket.java:49-51`, applied `:424` |
| INIT1 version bytes | `0C FF D2 FE` | `LocalClientHandlerConnecting.java:28` |
| identity-file XOR key | 106-char ASCII string | `LocalIdentity.java:36-39` |
| license root key | 32-byte Ed25519 public key | `License.java:12-18` |
| default `client_version` | `3.?.? [Build: 5680278000]` | `LocalClientHandlerConnecting.java:240` |
| default `client_version_sign` | 64-byte base64 signature | `LocalClientHandlerConnecting.java:251` |
| default `hwid` | `+LyYqbDqOvEEpN5pdAbF8/v5kZ0=` | `LocalClientHandlerConnecting.java:278` |

---

## 3. Handshake, in order

Extracted from `protocol/packet/handler/client/LocalClientHandlerConnecting.java`. The client is in
state `CONNECTING` and using the **bootstrap AES-EAX transform** (§2.1) for everything until
`setSecureParameters` is called. State machine: `DISCONNECTED → CONNECTING → RETRIEVING_DATA →
CONNECTED` (`protocol/client/ClientConnectionState.java:4-8`), handler chosen per state at
`LocalTeamspeakClientSocket.java:180-194`.

| # | Direction | Packet / command | Mandatory parameters | Code |
|---|---|---|---|---|
| 1 | C→S | `INIT1` type 0x8, **UNENCRYPTED**, MAC=`TS3INIT1`, version `0C FF D2 FE`, step **0** | `uint32 timestamp`(sec), `uint32 random`, 8 reserved zero bytes | `:37-54`, body `PacketBody8Init1.java:130-183` |
| 2 | S→C | `INIT1` step **1** | `serverStuff[16]`, `a0reversed[4]` | `:70-72` |
| 3 | C→S | `INIT1` step **2** | echoes `serverStuff` + `a0reversed` (nonce is sanity-checked reversed, mismatch only logs a warning) | `:74-88` |
| 4 | S→C | `INIT1` step **3** — RSA puzzle | `x[64]`, `n[64]`, `int32 level`, `serverStuff[100]` | `:89-99`, body `PacketBody8Init1.java:276-341` |
| 5 | C→S | `INIT1` step **4** — puzzle solution + inline command | `x`,`n`,`level`,`serverStuff`,`y[64]` where `y = x^(2^level) mod n` zero-padded left to 64 B, then **the `clientinitiv` command text as raw bytes** | `:101-126` (math at `:106-113`, inline command at `:124`) |
| 6 | (server may answer `INIT1` step **127**) | retry marker | client sleeps 1 s and restarts at step 0 | `:127-134` |
| 7 | S→C | command **`initivexpand`** *or* **`initivexpand2`** (still bootstrap crypto) | see §3.4 | `:140-212` |
| 8 | C→S | (only `initivexpand2`) command **`clientek`** | `ek`=base64(ephemeral Ed25519 pubkey), `proof`=base64(ECDSA-P256-SHA256(identity, ek‖beta)) | `:182-195` |
| 9 | C→S | command **`clientinit`** — sent **after** `setSecureParameters` (real keys) | see §3.5 | `:231-285` |
| 10 | S→C | command **`initserver`** | client reads `aclid` (client id), `acn` (nickname), `virtualserver_id` | `LocalTeamspeakClientSocket.java:772-787` |
| 11 | S→C | commands **`channellist`** (repeated) then **`channellistfinished`** | `channellistfinished` flips state to `CONNECTED` | `LocalTeamspeakClientSocket.java:73-82`, `:752-762` |

Notes / gotchas:
- Steps 1-5 are the whole `INIT1` exchange; the puzzle is validated only within
  `0 ≤ level ≤ 1_000_000` (`:98-99`) and the *server* decides the level. `test/GenerationTest.java`
  and the TS6 server log **[outside ts3j]** (`testbed/ts6-server/logs/…log`: "Precomputing puzzle…")
  confirm this is the real mechanism.
- The `clientinitiv` is **not** a normal command packet: it is embedded as trailing bytes of INIT1
  step 4 (`:124`), i.e. sent unencrypted inside the INIT1 payload. Parameters built at `:215-229`:
  `alpha` = 10 random bytes, `omega` = base64 identity public key DER, `ot` = `1` (constant),
  `ip` = **`InetAddress.getHostAddress().substring(1)`** (`:224-226`) — note the `substring(1)`, which
  drops the first character of the address string (e.g. `192.168.0.5` → `92.168.0.5`). Flag this when
  comparing against a real client capture.
- `alpha` is kept in a field and reused by `cryptoInit2` (`:197`, field at `:31`) — the ephemeral
  `alpha` from `clientinitiv` is the client's nonce for the license-based exchange.

### 3.1 Where the *server* version is negotiated
**NOT IMPLEMENTED / not found.** No code parses, checks or stores a server version. `initserver` is
reduced to three fields (`LocalClientHandlerConnecting`/`LocalTeamspeakClientSocket.java:772-787`);
the only "version" the client knows is its *own*. `api/Version.java:31-47` is a plain DTO
(`version`/`build`/`platform`) with no producer in the repo, and `VirtualServerInfo.getVersion()`
(`api/VirtualServerInfo.java:344`) is only a passive response-field accessor.
There is **no version gate, no minimum-build check, no `server_version` handling** anywhere.

### 3.2 How `client_version_sign` is computed
**NOT IMPLEMENTED / not found.** There is no signing/derivation code: the value is read from an option
with a hard-coded fallback (`LocalClientHandlerConnecting.java:247-252`) and can only be *injected*
wholesale via `setClientVersion(platform, version, sign)` → three `client.*` options
(`LocalTeamspeakClientSocket.java:1220-1224`). Same for `client_version` and `client_platform`
(`LocalClientHandlerConnecting.java:236-246`). No hash of the build string, no RSA/ECDSA over the
version — just a constant lifted from a capture.

### 3.3 `clientek` details
- `keyPair = Ts3Crypt.generateKeypair()` → **ephemeral Ed25519** (32-byte clamped scalar, pubkey via
  `ge_scalarmult_base`) (`util/Ts3Crypt.java:272-288`).
- `proof = ECDSA-P256-SHA256( identityPrivate , ek[32] ‖ beta[54] )` — a fixed **86-byte** message
  (`generateClientEkProof`, `util/Ts3Crypt.java:143-151`; 86 = 32+54).
- The server-side proof over the license blob is verified first:
  `verifySignature(decodePublicKey(omega), license, proof)` else `SecurityException`
  (`LocalClientHandlerConnecting.java:177-180`). Self-test:
  `test/ClientEkTest.java:41-43`.
- Then the session key uses the **ephemeral private key** (`keyPair.getValue()`), not the identity key
  (`:197`).

### 3.4 The two server-initiated branches
```
initivexpand   (alpha,beta,omega)                -> cryptoInit  -> setSecureParameters -> clientinit
initivexpand2  (ot==1, l,beta,omega,proof)       -> verify proof -> clientek -> cryptoInit2 -> clientinit
```
`ot != 1` is a hard `IllegalArgumentException` (`:159-160`); `beta` must be 10 or 54 bytes
(`util/Ts3Crypt.java:72-73`, `:163-164`). After `clientinit` the client sets
`setCommandProcessor(this)` and `setState(RETRIEVING_DATA)` (`:154-155`, `:203-204`).
Unknown init command → `IOException("Unknown Init command: …")` (`:209-210`); `error` → disconnect
(`:205-208`).

### 3.5 `clientinit` — exact parameter set sent
In order (`LocalClientHandlerConnecting.java:231-280`); the builder emits `key=escaped(value)` pairs
space-separated (`Command.build`, `command/Command.java:45-54`):

| parameter | source / default |
|---|---|
| `client_nickname` | `client.getNickname()` |
| `client_version` | option, default `3.?.? [Build: 5680278000]` |
| `client_platform` | option, default `Windows` |
| `client_version_sign` | option, default the 64-byte base64 constant |
| `client_input_hardware` | `1` (constant) |
| `client_output_hardware` | `1` (constant) |
| `client_default_channel` | option (may be absent → parameter dropped, see below) |
| `client_default_channel_password` | option |
| `client_server_password` | option (set at `connect()`, `LocalTeamspeakClientSocket.java:391`) |
| `client_nickname_phonetic` | option |
| `client_meta_data` | `""` |
| `client_default_token` | option (privilege key) |
| `client_key_offset` | identity key offset (10-decimal) |
| `hwid` | option, default `+LyYqbDqOvEEpN5pdAbF8/v5kZ0=` |

A `CommandSingleParameter` whose value is `null` serialises to the empty string and is therefore
*skipped* when building (`command/parameter/CommandSingleParameter.java:19-24`), so absent options
simply do not appear.

### 3.6 What a TS6 server can plausibly reject (all inference, from the facts above)
1. **`client_version` = `3.?.? [Build: 5680278000]`** — a non-numeric version. If the TS6 server gates
   on "TS3 client ≥ 3.6.0" (the stated acceptance rule), this string cannot satisfy a semantic version
   comparison. Mitigation exists: `setClientVersion(...)` (`LocalTeamspeakClientSocket.java:1220`).
   Confidence: high (it is the only version the server ever sees).
2. **`client_version_sign` is a stale constant that does not correspond to the version string sent.**
   Any server-side check that the signature covers the claimed build fails. Mitigation: supply a
   matching pair via `setClientVersion`, or accept whatever the server does about it.
   Confidence: medium (the repo contains no evidence either way about server validation).
3. **`hwid` constant** shared by every ts3j deployment — a per-client uniqueness/anti-abuse check would
   see collisions. Override via `setHWID` (`LocalTeamspeakClientSocket.java:809-811`).
4. **`ip` parameter mangled by `substring(1)`** (`LocalClientHandlerConnecting.java:226`) — if TS6
   validates or logs the client address, the value is wrong.
5. **No `client_mtu`/no MTU parameter at all** (§1.4) — if TS6 expects one, nothing is sent.
6. **`initivexpand` (legacy) is unauthenticated** — a TS6 server that only offers `initivexpand2` is
   fine; a server that offers `initivexpand` will be accepted without proof verification.
7. The **state machine depends on the server pushing `channellistfinished`**: no code in the repo ever
   *sends* a `channellist` request during the handshake (grep of `src/main`: only the two
   `executeCommand` senders at `LocalTeamspeakClientSocket.java:452-457` and `:1138-1143`, both
   user-invoked), yet `CONNECTED` is reached only on `channellistfinished` (`:752-762`). If TS6 does
   not push it unsolicited, `connect()` hangs until timeout. Unverifiable from the repo — **must be
   measured** against the TS6 server in `testbed/`.

---

## 4. Command layer

### 4.1 Encoding / escaping
- Wire syntax: `label [-flag]... key=value ...`, rows in one packet separated by `|`
  (`command/Command.java:39-54` for `build`, `command/MultiCommand.java:60-82` for multi-row build).
- Escaping (`util/Ts3String.java:4-24`), applied to **values only** (`CommandSingleParameter.toString`,
  `command/parameter/CommandSingleParameter.java:18-24`):

  | char | escape |
  |---|---|
  | `\` | `\\` |
  | `/` | `\/` |
  | space | `\s` |
  | `|` | `\p` |
  | `\f` | `\f` |
  | `\n` | `\n` |
  | `\r` | `\r` |
  | `\t` | `\t` |

  `\v` (vertical tab) is **commented out** in both directions (`:17`, `:34`) — a literal VT is emitted
  raw and would be an invalid escape to a strict parser. `unescape` **throws** on any unknown escape
  (`:43-45`).
- Parsing (`command/MultiCommand.java:84-137`): split off the label at the first space
  (`:85-86`); if the first token contains `=`, the whole text is treated as parameters with an **empty
  label** (`:88-91`); rows split on `|` (`:93`); parameters split on spaces (`:98`); `-x` → boolean
  option (`:106-107`); `k=v` → key lower-cased + value unescaped (`:109-118`); **duplicate keys within
  a row throw** (`command/SingleCommand.java:45-53`); every row inherits the first row's parameters
  (`MultiCommand.java:124-127`), which is how servers send static params once and per-row changing ones.
- Note: values are `trim()`ed before escaping (`CommandSingleParameter.java:23`), so leading/trailing
  spaces in payloads are lost.
- `PacketBody2Command` writes UTF-8 bytes and uses UTF-8 byte length for `getSize()`
  (`protocol/packet/PacketBody2Command.java:44-52`); `PacketBody3CommandLow.getSize()` returns
  `text.length()` — **characters, not bytes** (`PacketBody3CommandLow.java:49-52`): an outbound
  COMMAND_LOW with non-ASCII would be mis-sized. (ts3j never sends COMMAND_LOW — see §4.4.)

### 4.2 Requests, ordering, replies
- Every user command goes out as `COMMAND` (0x2) with the `NEW_PROTOCOL` flag
  (`AbstractTeamspeakClientSocket.java:312-315`) and a **`return_code=<n>`** parameter injected by
  `executeCommand` (`LocalTeamspeakClientSocket.java:478-499`).
- Response correlation is **FIFO by code, not by the echoed value**: the client executes the lowest
  pending `return_code` and, on completion, sends the next one
  (`getAwaitingCommandCode` `:562-573`, `sendNextCommand` `:583-589`, completion at `:703-727`).
  The server's `return_code` echo is never read. This is a head-of-line dependency: commands are
  strictly serialised per connection.
- Reply parsing: each inbound `COMMAND`/`COMMAND_LOW` is parsed into a `MultiCommand`
  (`protocol/packet/handler/client/LocalClientHandlerFull.java:28-56`). The terminal `error` row
  (`id`, `msg`) closes the pending command: `id == 0` → success with the collected rows, else
  `CommandException(msg, id)` (`LocalTeamspeakClientSocket.java:703-727`). Rows are accumulated until
  `error` arrives (`:685-701`).
- Command text is unbounded but COMMAND is splittable (≤500 B fragments, §1.4) and QuickLZ-compressible.

### 4.3 Notify events
`LocalTeamspeakClientSocket.process(SingleCommand)` (`:429-450`):
1. exact-name `namedProcessors` lookup first (`:71-149`: `initserver`, `channellist`,
   `channellistfinished`, `notifyclientleftview`, `notifyconnectioninforequest`);
2. else any name starting with `notify` → `TS3Event.createEvent` and fan-out to listeners
   (`:435-437`);
3. else route to the pending command response, else drop with a log line (`:439-448`).

`event/TS3Event.java:34-93` maps (complete list):

`notifytextmessage`→TextMessageEvent, `notifycliententerview`→ClientJoinEvent,
`notifyclientleftview`→ClientLeaveEvent, `notifyserveredited`→ServerEditedEvent,
`notifychanneledited`→ChannelEditedEvent,
`notifychanneldescriptionchanged`→ChannelDescriptionEditedEvent,
`notifyclientmoved`→ClientMovedEvent, `notifychannelcreated`→ChannelCreateEvent,
`notifychanneldeleted`→ChannelDeletedEvent, `notifychannelmoved`→ChannelMovedEvent,
`notifychannelpasswordchanged`→ChannelPasswordChangedEvent, `notifytokenused`→PrivilegeKeyUsedEvent,
`notifyservergrouplist`, `notifychannelgrouplist`, `notifyclientchannelgroupchanged`,
`notifyclientneededpermissions`, `notifyclientupdated`, `notifychannelsubscribed`,
`notifychannelunsubscribed`, `notifyservergroupclientadded`, `notifyservergroupclientdeleted`,
`notifyclientpoke`, `notifyclientchatcomposing`, `notifyclientchatclosed`, `notifypermissionlist`,
`notifyclientpermhints`, `notifychannelpermhints` → respective event classes
(`event/*Event.java`, one class each).
Unknown names → `UnknownTeamspeakEvent(name, params)` (`:90-92`) — **new TS6 notifications degrade
gracefully** instead of failing.
Events are dispatched on a dedicated executor (default single-thread, `:47`, `:152-170`); parallel
dispatch is documented as "dangerous" and out-of-order (`:156-164`).
Special-cased: `notifyclientleftview` for *our own* client id flips state to `DISCONNECTED` and emits
`DisconnectedEvent` (`:84-104`); `notifyconnectioninforequest` triggers a `setconnectioninfo` reply
carrying ping/packet statistics (`:106-149`).

### 4.4 Commands implemented (selection, all in `LocalTeamspeakClientSocket.java`)
`clientinit`, `clientinitiv`, `clientek` (handshake); `channellist`, `clientlist`, `channelinfo`,
`clientinfo`, `channelsubscribeall`, `clientmove`, `clientupdate`, `clientedit`, `clientpoke`,
`clientkick`, `clientdisconnect`, `sendtextmessage` (targetmode 1/2/3), `banadd`, `banclient`,
`banlist`, `deleteban`, `bandelall`, `clientaddperm`, `clientdelperm`, `channeladdperm`,
`channelclientaddperm`, `channeldelperm`, `servergroupaddclient`, `servergroupdelclient`.
Unimplemented/known-broken: `listBans` returns via `notifybanlist` — commented
`// TODO notifybanlist is returned instead` (`:922-927`); `listChannelClients` — `// TODO command not
found` (`:1057-1065`); `channelClientAddPermission` passes `cid` as `cldbid` (`:1017-1018`) and
`channelClientDeletePermission` sends `channeldelperm` instead of `channelclientdelperm`
(`:1023-1040`) — copy-paste defects.
`COMMAND_LOW` is **never sent** (only `PacketBody2Command` is constructed for output:
`LocalTeamspeakClientSocket.java:742`, `:1281-1293`; handler `LocalClientHandlerFull.java:44-56`).

---

## 5. Voice path

### 5.1 Codec
- **The library contains no codec.** `audio/Microphone` is a 3-method callback interface
  (`isReady/getCodec/provide`, `audio/Microphone.java:5-14`) and `audio/AudioEncoding`
  (`OPUS, SPEEX, PCM, CELT`, `audio/AudioEncoding.java:3-8`) is unused by the transport.
- Codec ids on the wire (`enums/CodecType.java:3-13`): `0 SPEEX_NARROWBAND, 1 SPEEX_WIDEBAND,
  2 SPEEX_ULTRA_WIDEBAND, 3 CELT_MONO, 4 OPUS_VOICE, 5 OPUS_MUSIC, 127 RAW`.
- Opus is used only in `examples/audio` via JNA (`net.tomp2p:opus-wrapper:1.4`, plus JNA 3.0.9,
  `ex/pom.xml:44-55`): the example sink's `AudioFormat` is **48000 Hz / 32-bit float / 2 channels**
  (`ex/TeamspeakFastMixerSink.java:14-15`, asserted 48 kHz at `:57-59`), permitted Opus frame sizes are
  120/240/480/960/1920/2880 samples and sample rates 8000/12000/16000/24000/48000
  (`ex/OpusEncoder.java:16-20`, validated `:51-58`), the example encodes **20 ms** frames
  (`ex/AudioSender.java:25-33`: `OpusParameters(20, 96000, complexity 10, loss 0, VBR off, FEC off,
  music=true)`), and it sets `OPUS_SIGNAL_MUSIC` vs `VOICE`, bitrate, complexity, VBR and in-band FEC
  on the encoder (`ex/TeamspeakFastMixerSink.java:202-220`).
- Consequence for us: the packet carries a **single Opus packet** per datagram; whether stereo/`OPUS_MUSIC`
  is legal in a given channel is a *channel* property (`api/Codec`, `api/ChannelInfo`), not something
  the transport enforces.

### 5.2 Voice packet format
`protocol/packet/PacketBody0Voice.java`:
```
client -> server : uint16 packetId | uint8 codecType | codecData[len] | [uint8 sessionFlag]
server -> client : uint16 packetId | uint16 clientId | uint8 codecType | codecData[len] | [uint8 sessionFlag]
```
(`write` `:105-118`, `read` `:75-103`, sizes `:120-127`.) All big-endian.
- The optional trailing `sessionFlag` byte exists **only when the `COMPRESSED` header flag is set**
  (`read` `:79-93`, `setHeaderValues` `:69-73`). For voice, `COMPRESSED` is *not* a compression marker:
  it is reused as the "voice-session start" marker. README: "the first 5 packets starting a voice
  session are marked with the COMPRESSED flag" (`README.md:131`).
- `sessionFlag` values 1..7 cycle per session; the client sets it for the first **5** packets and
  increments the session byte when the stream stops (`LocalTeamspeakClientSocket.java:593-594`,
  `:619`, `:624-625`, `:641-647`). The field's meaning is explicitly unknown:
  `PacketBody0Voice.java:17-22` ("Unknown at this time (have seen: 0x01..0x07) … Best to ignore it").
- **End of stream = a zero-length `codecData` packet** (`LocalTeamspeakClientSocket.java:631-652`),
  confirmed against a captured packet: `test/VoicePacketTest.java:24-32`
  (`codecType=OPUS_MUSIC, codecData.length==0`) and README `:131`.
- Note that same captured packet has the header `UNENCRYPTED` bit set (`test/VoicePacketTest.java:24`,
  byte `0x80`) — official clients send voice **unencrypted**; ts3j encrypts it, because it never sets
  `UNENCRYPTED` for VOICE (`AbstractTeamspeakClientSocket.java:305-316`). Both are accepted by the
  servers tested, but it differs from the reference client.

### 5.3 Send loop, timing, sequence
- `MicrophoneTask` is driven by `HighPrecisionTimer(period=20 ms, catchupTicks=5.0, …)` i.e. **50 Hz
  poll with up to 100 ms drift compensation** (`LocalTeamspeakClientSocket.java:521-534`; timer
  implementation `util/HighPrecisionTimer.java:37-77`, busy-wait fallback at `:71-75`).
- Each tick: if `microphone.isReady()` → wrap `microphone.provide()` bytes into a VOICE packet with
  `codecType = microphone.getCodec()` and send (`:610-630`); if it was sending and the mic reports not
  ready → send the zero-length terminator and roll the session id (`:631-652`).
- The sequence number is the **VOICE-type packet counter** copied from the header
  (`AbstractTeamspeakClientSocket.java:407-411`). There is **no timestamp in the voice body and no
  jitter buffer**; consumers "will need to decode the packets yourself, and insert
  packet-loss-correction as needed" (`README.md:129`).
- Voice is **not** resendable and **not** splittable (`PacketBodyType.java:6-7`) and is capped at 13+1
  bytes of header/overhead → ≤486 bytes of Opus payload per datagram.
- A VOICE packet carries **no channel id** — routing is the server's job, based on the sender's current
  channel; the receiving side learns the speaker from the body `clientId` (server→client form,
  `PacketBody0Voice.java:99-100`).

### 5.4 Whisper
`protocol/packet/PacketBody1VoiceWhisper.java`: same as VOICE plus a target block after the codec byte,
sent by the client only (`read` `:47-82`, `write` `:90-106`).
- `NEW_PROTOCOL` set ⇔ target is a **group whisper** (`setHeaderValues` `:36-45`).
- Group target: `uint8 whisperType | uint8 whisperTarget | int64 targetId` (`WhisperTargetGroup`
  `:254-301`; enums `enums/GroupWhisperType.java`, `enums/GroupWhisperTarget.java`).
- Multi target: `uint8 nChannels | uint8 nClients | int64 channelIds[] | uint16 clientIds[]`
  (`WhisperTargetMultiple` `:187-252` — note `read` loops the *client* array with `channelIdCount`,
  `:242-243`, a bug on inbound multi-whisper).
- Helpers `createChannel/createClient/createServer` (`:303-322`).

### 5.5 "Latency" mode
**NOT IMPLEMENTED / not found.** The only occurrence of latency in the whole repo is the plain
response-field accessor `ChannelInfo.getCodecLatencyFactor()` (`api/ChannelInfo.java:53`, reading a
`codec_latency_factor`-style property). There is no voice-packet latency flag, no latency/jitter mode
negotiation, no `client_mtu`/`codec_latency` command in `src/main`.

### 5.6 PTT vs voice activation
**Delegated entirely to the `Microphone` implementation** (`audio/Microphone.java:5-14`):
`isMuted()` defaults to `false`, `isReady()` decides "should we put audio on the wire now". So PTT/VAD
is *whatever the caller's `provide()`/`isReady()` does*. The library only adds:
- a `// TODO` for muted state/events (`LocalTeamspeakClientSocket.java:607-609`) — **muted is not
  transmitted to the server at all**;
- `client_input_hardware`/`client_output_hardware` are hard-coded `1` in `clientinit`
  (`LocalClientHandlerConnecting.java:253-254`) — no `input_muted`/`output_muted` command support;
- the 20 ms poll and the 5-packet session marker (§5.3).

---

## 6. File transfer

**NOT IMPLEMENTED / not found.** Verified by exhaustive search:
- No TCP client anywhere: the only socket is `new DatagramSocket()`
  (`LocalTeamspeakClientSocket.java:371`); `java.net.Socket`, `SocketChannel`, `ServerSocket` do not
  appear in `src/main` (the only `Socket` hits are `SocketRole`/`DatagramSocket`/`SocketTimeoutException`).
- No `ftinit…` command is ever built: grep for `ftinit|ftinitupload|ftinitdownload` over the whole
  repo returns nothing in `src`.
- Port 30033 exists only as an unused constant (`protocol/TS3DNS.java:15`), likewise query `10011`
  (`:14`).
- What does exist are **response DTOs copied from the old TS3 Java API**:
  - `api/FileTransferParameters.java:37-126` — models `clientftfid`, `serverftfid`, `ftkey`, `ip`,
    `port`, `size`, `status`, `msg` (the reply to `ftinitupload`/`ftinitdownload`).
  - `api/FileTransfer.java:31-166` — models a `notifyfiletransferlist` row: `clid`, `path`, `name`,
    `size`, `sizedone`, `clientftfid`, `serverftfid`, `status`, `current_speed`, `average_speed`,
    `runtime`.
  - `api/FileListEntry.java`, `api/FileInfo.java`, `api/IconFile.java` — DTOs only.
- Nothing constructs these DTOs from real traffic and nothing consumes `ftkey`/`ip`/`port`.
- Therefore: **the TCP 30033 handshake (`ftkey`, transfer key as a 4×uint32 id, size header, raw byte
  stream) is NOT IMPLEMENTED and cannot be re-derived from this repo.**
- **[outside ts3j]** For a TS6 client this may be moot: the TS6 changelog lists "S3 migration
  adjustments (preparation for client release)" and "Fixed S3 file transfer reply handling"
  (`testbed/ts6-server/CHANGELOG.txt:21`, `:40`), and the server log still shows
  `FileManager listening on 0.0.0.0:30033` (`testbed/ts6-server/logs/…_0.log`). I.e. "classic
  30033" still listens, but file transfers in TS6 appear to have moved to S3-backed storage — reading
  the changelog that way is an **inference**, not a repo fact.

---

## 7. Code map (navigate fast)

| # | Class / method | Purpose | Location |
|---|---|---|---|
| 1 | `LocalTeamspeakClientSocket.connect()/process()/executeCommand()` | public client: connect, route inbound commands, serialised command execution with `return_code` | `protocol/socket/client/LocalTeamspeakClientSocket.java:349`, `:429`, `:472` |
| 2 | `AbstractTeamspeakClientSocket` | the protocol core: fragmentation, encryption, counters, ACK/resend, RTT, read loop | `protocol/socket/client/AbstractTeamspeakClientSocket.java:44` |
| 3 | `AbstractTeamspeakClientSocket.readPacket()` | inbound pipeline: dedupe → decrypt → decompress → auto-ACK/PONG → reassemble | `:609-793` |
| 4 | `writePacketIntl()` | outbound pipeline: counter → encrypt/fake-encrypt → datagram → await ACK | `:363-515` |
| 5 | `PacketTransformation.encrypt()/decrypt()/computeParameters()` | AES-128-EAX + per-packet key/nonce derivation | `protocol/packet/transformation/PacketTransformation.java:35`, `:65`, `:122` |
| 6 | `InitPacketTransformation` | fixed-key bootstrap crypto | `protocol/packet/transformation/InitPacketTransformation.java:8-21` |
| 7 | `LocalClientHandlerConnecting` | **entire handshake**: INIT1 0/2/4, initivexpand(2), clientek, clientinit | `protocol/packet/handler/client/LocalClientHandlerConnecting.java:37-285` |
| 8 | `PacketBody8Init1` + `Step0..Step127` | INIT1 framing and RSA puzzle structs | `protocol/packet/PacketBody8Init1.java:12-459` |
| 9 | `Ts3Crypt` | all crypto primitives: SHA helpers, ECDH, license key derivation, clientek proof, DER pubkey codec | `util/Ts3Crypt.java:30-337` |
| 10 | `Identity` / `LocalIdentity` / `Uid` | P-256 identity, uid derivation, security-level grinding, TS3 ini import/export | `identity/Identity.java:17`, `identity/LocalIdentity.java:27`, `identity/Uid.java:6` |
| 11 | `License` + `LicenseUse*` | TS3 license-block parser and Ed25519 chain key derivation | `license/License.java:11-233`, `license/LicenseUseType.java:3-8` |
| 12 | `MultiCommand.parse()/build()`, `SingleCommand`, `Ts3String` | command codec: label/params/rows and the escape table | `command/MultiCommand.java:84`, `:60`, `util/Ts3String.java:4` |
| 13 | `PacketBodyType` | the authoritative packet-type table (ids + capabilities) | `protocol/packet/PacketBodyType.java:6-14` |
| 14 | `PacketHeader` / `ClientPacketHeader` / `ServerPacketHeader` / `HeaderFlag` | wire header layout (13 vs 11 bytes) and flags | `protocol/header/*.java` |
| 15 | `Fragments.split()` / `PacketReassembly` | 500-byte MTU split, QuickLZ, first/last-fragment markers, reassembly | `protocol/packet/fragment/Fragments.java:28`, `PacketReassembly.java:24` |
| 16 | `PacketBody0Voice` / `PacketBody1VoiceWhisper` | voice + whisper body formats and whisper targets | `protocol/packet/PacketBody0Voice.java:11`, `PacketBody1VoiceWhisper.java:12` |
| 17 | `LocalTeamspeakClientSocket.MicrophoneTask` | 50 Hz mic poll → VOICE packets, 5-packet session marker, zero-length terminator | `LocalTeamspeakClientSocket.java:591-655` |
| 18 | `LocalClientHandlerFull` / `…RetrievingData` / `…Connected` | per-state packet dispatch (COMMAND, COMMAND_LOW, VOICE, WHISPER) | `protocol/packet/handler/client/LocalClientHandlerFull.java:28-67` |
| 19 | `TS3Event.createEvent()` | notify-name → event object mapping | `event/TS3Event.java:34-93` |
| 20 | `QuickLZ` (1.5.0, levels 1/3, 9-byte header) | the compression codec used when `COMPRESSED` is a real compression marker | `util/QuickLZ.java:26-28`, `:41-44`, `:72`, `:313` |

---

## 8. Maturity

### 8.1 What is proven to work
- Real end-to-end connection code and a manual smoke test against a live public server
  (`test/ServerConnectionTest.java:42-47` connects to `teamlixo.net` and asserts `CONNECTED`) — note
  this is a `main()`, not a JUnit test, and it needs the internet.
- Crypto pinned by known-answer vectors taken from an independent implementation
  ("The assertion check values SHOULD be retrieved from another known working codebase (i.e. Splamy's
  C# codebase)", `test/CryptoInitTest.java:14`, `test/CryptoInit2Test.java:10`,
  `test/LicenseDerivationTest.java:16`):
  | vector | site |
  |---|---|
  | `initivexpand` shared secret / ivStruct / fakeSig | `test/CryptoInitTest.java:37-49` |
  | `initivexpand2` ivStruct `E4082A92…F4513`, fakeSig `54F2B4D661E0F9AB` | `test/CryptoInit2Test.java:32-34` |
  | license chain derived key | `test/LicenseDerivationTest.java:42-47` |
  | uid + security level from a private key | `test/ClientUidGenerationTest.java:19-28` |
  | identity ini round-trip (uid + level 15) | `test/IdentityFileTest.java:18-37` |
  | AES-EAX per-packet key/nonce pairing | `test/EncryptionTest.java:47-50` |
  | real captured voice packet parse | `test/VoicePacketTest.java:24-32` |
  | fragmentation flags | `test/FragmentTest.java:52-115` |
  | received-packet window/generations | `test/PacketCounterTest.java:13-28` |
  | clientek proof sign+verify (self-consistency only) | `test/ClientEkTest.java:41-43` |
- Community usage is claimed in `README.md:11-20` (Manebot/JeakBot).

### 8.2 Untested / TODO / dead code
- TODOs: muted state never sent (`LocalTeamspeakClientSocket.java:607-609`), `banlist` returns via
  `notifybanlist` (`:922`), `channelclientlist` "command not found" (`:1057`).
- `PacketBody1VoiceWhisper.read(ByteBuffer)` throws `UnsupportedOperationException`
  (`PacketBody1VoiceWhisper.java:84-88`) — only the header-aware `read(PacketHeader,ByteBuffer)` works.
- `RemoteCounterFullModulated.getCurrentGeneration()` throws (`AbstractTeamspeakClientSocket.java:1339`);
  the class is never used.
- `CommandOption.set(*value*)` throws for everything except `boolean`
  (`command/parameter/CommandOption.java:22-59`) — no `-flag=value` support.
- Login/query layer (10011), file transfer, snapshots, icon upload are absent entirely.
- Server-side: `protocol/socket/server/TeamspeakServerSocket.java:6-10` declares only
  `TeamspeakClientSocket accept()` and **has no implementation anywhere in the repo**;
  `ProtocolRole.SERVER`/`SocketRole.SERVER` exist only as header/role plumbing for parsing
  server→client packets.
- Whisper: `PacketBody1VoiceWhisper` can be serialised, but **no library API ever sends one** — only
  the body structs, the target builders (`:303-322`) and the receive-side consumer
  (`LocalClientHandlerFull.java:62-65`) exist.

### 8.3 Test coverage reality
- 19 files in `src/test`; **17 extend `junit.framework.TestCase`** (JUnit 3 style) and therefore are
  picked up by Surefire via JUnit4's adapter; 2 are plain classes with only `main()`
  (`test/ServerConnectionTest.java:17`, `test/GenerateIdentityTest.java:7`) and are **never run by
  the build**.
- No `@Test` annotations anywhere; no mocking; no socket-level integration test; no test for
  `PacketBody8Init1`, `PacketTransformation.decrypt` against foreign ciphertext, `MultiCommand.parse`
  round-trip beyond `test/EncodingTest.java:27-30`, or the handshake state machine.
- `test/TS3DNSTest.java:17-22` performs a live DNS SRV lookup (network-dependent).
- Deps (`pom.xml:29-60`): junit 4.13.1 (test), bcprov-jdk15on 1.67, commons-lang 2.6, dnsjava 2.1.8,
  ini4j 0.5.1. Java 8 source/target. Version `1.0.3` (`pom.xml:9`).

### 8.4 Hard-coded constants that decide TS6 compatibility
| Constant | Value | Site | Why it matters |
|---|---|---|---|
| `client_version` | `3.?.? [Build: 5680278000]` | `LocalClientHandlerConnecting.java:240` | TS6 accepts TS3 clients ≥ 3.6.0; this is not a parseable version |
| `client_version_sign` | 64-byte base64 blob | `:251` | not derived from the version string; a validating server rejects |
| `client_platform` | `Windows` | `:245` | harmless but fake on every platform |
| `hwid` | `+LyYqbDqOvEEpN5pdAbF8/v5kZ0=` | `:278` | identical for every ts3j user |
| `INIT1_VERSION` | `0C FF D2 FE` | `:28` | the only client-protocol version marker sent |
| `MAXIMUM_PACKET_SIZE` | 500 | `Fragments.java:21` + `LocalTeamspeakClientSocket.java:203` | no MTU negotiation, hard recv cap |
| `TIMEOUT` | 30000 ms | `AbstractTeamspeakClientSocket.java:47` | 30 s silence ⇒ drop |
| resend | 30 tries × 1000 ms | `:471`, `:498` | 30 s worst case per command |
| mic period | 20 ms | `LocalTeamspeakClientSocket.java:522-526` | voice pacing |

---

## 9. GAPS / RISKS for a TS6 client

**Crypto/handshake (low risk, this is the good news)**
1. The `initivexpand2` scheme is fully specified here and pinned by KATs: license chain from a fixed
   Ed25519 root → 32-byte key → Ed25519 scalarmult with an ephemeral client key → SHA-512 →
   `alpha`/`beta` XOR → AES-128-EAX per packet. A TS3 ≥3.4 client uses exactly this shape, so a TS6
   server that accepts TS3 clients ≥ 3.6.0 should accept it. **Highest-confidence path.**
2. The legacy `initivexpand` branch is unauthenticated (no signature over `omega`/`alpha`/`beta`) —
   if TS6 offers it, we inherit that weakness; if it does not, ignore it.
3. `hash128` is SHA-1, not a 128-bit hash — easy reimplementation trap (`util/Ts3Crypt.java:35-51`).
4. The 8-byte EAX tag must be fed through CTR after the ciphertext (TS3's quirk), not compared
   directly (`PacketTransformation.java:105-150`); a "clean" AEAD implementation will not interoperate.

**Client identity / admission (medium risk)**
5. `client_version` + `client_version_sign` are **fake constants with no derivation code**. This is the
   single most likely TS6 rejection cause and the repo offers no fix beyond overriding the strings.
   We must obtain a real (version, sign, platform) triple for a TS3 3.6.x build.
6. `hwid` is a shared constant; if TS6 dedupes/bans on it, all clones collide.
7. `client_key_offset` (identity security level) is real and must be ≥ 0; `improveSecurity` is a busy
   loop with no upper bound (`identity/Identity.java:79-102`) — a TS6 server expecting level ≥ 8/10
   needs the grinding done offline/cached.

**Protocol behaviour (medium risk)**
8. **`channellistfinished` is assumed to be pushed by the server** (§3.6 item 7). Measure this against
   `testbed/ts6-server` before porting the state machine; the alternative (client must request
   `channellist`) is not implemented here.
9. 500-byte fixed receive buffer (`LocalTeamspeakClientSocket.java:203`): oversized datagrams are
   truncated with no error path. A TS6 server with a larger MTU (or a voice packet >500 B) silently
   breaks. Use a bigger buffer and an explicit MTU in a new client.
10. Reassembly requires *contiguous* packet ids within a type (`PacketReassembly.java:43-70`); a single
    lost fragment wedges that type's queue (no timeout, no re-request). Fragile under loss.
11. Command responses are matched **FIFO, ignoring the echoed `return_code`**
    (`LocalTeamspeakClientSocket.java:562-573`). Any out-of-order response (or a server that answers a
    different command first) desynchronises the pending map permanently.
12. No `COMMAND_LOW` sender, no `-flag` with values, `\v` escaping hole, `PacketBody3CommandLow`
    size bug (`text.length()` vs UTF-8 bytes) — a UTF-8-heavy client should fix all three.

**Missing subsystems (high effort)**
13. **File transfer: zero implementation.** If TS6 keeps TCP 30033, the entire `ftinit*` + key/size
    handshake has to be taken from another source (ts3j cannot supply it). If TS6 moved to S3
    (changelog hints), the work is different: HTTP(S) upload/download plus `ftgetchannelfilehttptoken`
    (`testbed/ts6-server/serverquerydocs/ftgetchannelfilehttptoken.txt` exists).
14. **Voice codecs**: none in the library. Opus must be integrated by us (the examples show JNA +
    libopus 48 kHz/20 ms mono, values matching TS3 defaults).
15. **No PTT/VAD, no muted state, no input/output mute commands** — if TS6 expects
    `clientupdate client_input_muted=…`, we must add it; the repo sends nothing.
16. **Latency mode / MTU negotiation: not implemented**, so if TS6 changes voice pacing or packet
    flags, we have no reference behaviour here.
17. **Whisper send path** exists only as target builders; the incoming multi-whisper reader is buggy
    (`PacketBody1VoiceWhisper.java:242-243`). Do not trust it as a reference for group whisper.

**Evidence hygiene**
18. The repo has exactly **one squashed commit** — no history, no issues, no design notes. Every claim
    above is read off the code, and the two `main()`-style tests cannot be executed offline. Nothing
    in this document is verified against a live TS6 server; the only TS6-side evidence collected is
    the server's own version banner and port bindings
    (`testbed/ts6-server/logs/…_0.log`: `TeamSpeak Server 6.0.0-beta13.1 (2026-09-22)`,
    UDP 9987, FileManager 30033).

---

## Appendix A — handshake at a glance

```
C  INIT1 step0  (ver 0CFFD2FE, ts, rand4, 8x00)            [plaintext, MAC "TS3INIT1"]
S  INIT1 step1  (serverStuff16, a0rev4)
C  INIT1 step2  (serverStuff16, a0rev4)
S  INIT1 step3  (x64, n64, level32, serverStuff100)          [RSA puzzle]
C  INIT1 step4  (x64, n64, level32, stuff100, y64, "clientinitiv alpha=… omega=… ot=1 ip=…")
S  "initivexpand alpha=… beta=… omega=…"                    [AES-EAX with the bootstrap key]
   |  or
S  "initivexpand2 ot=1 l=… beta=… omega=… proof=…"
C  "clientek ek=… proof=…"                                  [bootstrap key]
   -> derive ivStruct, install real transform
C  "clientinit client_nickname=… client_version=… client_version_sign=… hwid=… …"   [real AES-EAX]
S  "initserver aclid=… acn=… virtualserver_id=…"
S  "channellist …" | … | "channellistfinished"              [state -> CONNECTED]
```
`[plaintext]` = INIT1 MAC magic; `[bootstrap key]` = `c:\windows\system` / `m\firewall32.cpl`.

## Appendix B — minimal facts a reimplementation must match

| Fact | Value | Cite |
|---|---|---|
| header sizes | 13 B (C→S) / 11 B (S→C), MAC first, big-endian | `ClientPacketHeader.java:55`, `ServerPacketHeader.java:39` |
| type ids | 0 VOICE, 1 WHISPER, 2 COMMAND, 3 COMMAND_LOW, 4 PING, 5 PONG, 6 ACK, 7 ACK_LOW, 8 INIT1 | `PacketBodyType.java:6-14` |
| flags | 0x10 FRAG, 0x20 NEW_PROTO, 0x40 COMPRESSED, 0x80 UNENCRYPTED | `HeaderFlag.java:3-9` |
| AEAD | AES-128-EAX, 8-byte tag, AAD = header minus MAC | `PacketTransformation.java:27,79-84` |
| key/nonce | `SHA256( roleByte ‖ type ‖ int32(generation) ‖ ivStruct )`, key=H[0..16), nonce=H[16..32), key[0..1] ^= packetId | `PacketTransformation.java:35-63` |
| uid | `b64(sha1(ascii(b64(DER(pubkey)))))` | `Identity.java:149-153` |
| identity curve | P-256 (`prime256v1`), ECDSA-SHA256 | `LocalIdentity.java:77-94`, `Ts3Crypt.java:203-226` |
| clientek curve | Ed25519 (ephemeral), proof = ECDSA over `ek‖beta` (86 B) | `Ts3Crypt.java:143-151,272-288` |
| MTU | 500 bytes, no negotiation | `Fragments.java:21` |
| keepalive | PING ≤1 Hz, drop at 30 s | `AbstractTeamspeakClientSocket.java:47,622-636` |
| voice EOS | zero-length codecData | `LocalTeamspeakClientSocket.java:631-652` |
| compression | QuickLZ 1.5.0 level 1, header 9 B | `QuickLZ.java:26-28,41-44,72` |
