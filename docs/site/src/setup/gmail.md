# Gmail access

*Verified against: `unaudited`*

hauz reads mail from **one Gmail label** through the Gmail API with the read-only scope
(`gmail.readonly`). It never relabels, marks as read or deletes mail. Variables:
[Environment reference](env.md).

## One-time Google Cloud setup
1. In the Google Cloud project, enable the Gmail API.
2. Create an OAuth client of type **Desktop app**. Note its client id and secret.
3. Set the OAuth consent screen to **In production**. In Testing mode, refresh tokens for this
   scope expire after 7 days. For personal use, accept the unverified-app warning.
4. In Gmail, create the label (e.g. `bills`) and a filter that applies it to bill senders.
   To backfill, label older bills by hand.

## Getting a refresh token
```
export HAUZ_GMAIL_CLIENT_ID=<CLIENT_ID> HAUZ_GMAIL_CLIENT_SECRET=<CLIENT_SECRET>
export "$(hauz gmail-auth)"        # sets HAUZ_GMAIL_REFRESH_TOKEN in this shell
```
`hauz gmail-auth` prints a consent URL on stderr. Open it in a browser on the same machine
and approve. The browser is redirected to a temporary listener on `127.0.0.1:<random
port>`, and the command prints exactly one stdout line, `HAUZ_GMAIL_REFRESH_TOKEN=<token>`.
The full exchange is in [the gmail-auth sequence](../sequences/gmail-auth.md).

- **Exit 1:** a missing client id or secret (stderr names it), declined consent, a
  mismatched `state`, a token-endpoint error, or a response without a refresh token.
- **No timeout:** the command waits for the redirect until you press Ctrl-C.
- **Revoking:** at myaccount.google.com/permissions.

## Using it
- `hauz fetch` lists the label, optionally bounded by `--after`/`--before` UTC days
  ([CLI reference](../reference/cli.md)).
- The server polls the label when `HAUZ_GMAIL_CLIENT_ID` is set
  ([sequence](../sequences/fetch-poll.md)).

**First real run:** fetch the same range twice. The second run must print only
`Duplicate`, which confirms that Gmail returns the same raw bytes every time.
