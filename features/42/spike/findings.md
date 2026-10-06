# Spike #42 findings (no credentials; bogus creds only)

## 1. Token exchange
POST https://oauth2.googleapis.com/token, form: client_id, client_secret, refresh_token, grant_type=refresh_token.
Keep client_secret (Desktop clients get one). Success: `{access_token,expires_in,scope,token_type}`.
```
$ curl -d client_id=bogus.apps.googleusercontent.com -d client_secret=bogus -d refresh_token=1//bogus -d grant_type=refresh_token https://oauth2.googleapis.com/token
{"error":"invalid_client","error_description":"The OAuth client was not found."}  HTTP 401
```
Same without secret. Revoked token not distinguishable without a real client; documented, UNVERIFIED:
400 `{"error":"invalid_grant","error_description":"Token has been expired or revoked."}`. Both map to Auth.

## 2. Gmail error (real)
Bogus Bearer on `/gmail/v1/users/me/messages?q=label:bills`: HTTP 401
`{"error":{"code":401,"errors":[{"reason":"authError"}],"status":"UNAUTHENTICATED"}}`. Any 401 => Auth.

## 3. Discovery doc (real)
list: q, pageToken, maxResults (default 100, max 500). Response: messages[{id,threadId}], nextPageToken,
resultSizeEstimate. Empty result likely omits `messages` (unverified): `serde(default)`.
get format=raw: `raw` is base64url, usually unpadded; accept both.
Unverifiable: byte identity of raw vs original. Not load-bearing offline; first real run should fetch twice and compare hashes.

## 4. Dependencies
base64 is in the tree twice (0.22.1, 0.23.1): hand-roll std-only base64url decoder.
reqwest 0.13 with `default-features=false, features=["json","rustls"]` adds zero crates to Cargo.lock (verified with a
throwaway crate). Needs decisions.md entry.

## 5. Test seam
Workspace axum/tokio as core dev-deps: no new crates (check tokio `net` feature). Sketch: ref/mail.rs;
`GmailSource::with_endpoints(creds, token_url, api_base)`.

## 6. gmail-auth
~150-200 lines, live consent unverifiable offline, PKCE needs sha256/random (dependency decision). Recommend SPLIT
into its own issue; #42 keeps MailSource, GmailSource, base64url, `hauz fetch`.

## Unknowns
Live: empty-list shape, raw stability, refresh on mid-paging 401.

<!-- STATUS: COMPLETE -->
