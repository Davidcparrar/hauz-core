# Gmail access

*Verified against: `unaudited`*

hauz reads the mail of **one Gmail label** with the Gmail API. It uses the read-only scope
(`gmail.readonly`). hauz does not change a label, does not mark mail as read and does not
delete mail. For the variables, see the [Environment reference](env.md).

## One-time Google Cloud setup
1. In the Google Cloud project, enable the Gmail API.
2. Create an OAuth client of the type **Desktop app**.
3. Write down the client id and the secret of the client.
4. Set the OAuth consent screen to **In production**.

   > **NOTE:** In Testing mode, the refresh tokens for this scope stop after 7 days.
5. For personal use, accept the warning about the unverified app.
6. In Gmail, create the label, for example `bills`.
7. Create a filter that puts the label on the mail from bill senders.
8. To include older bills, put the label on them manually.

## Get a refresh token
```
export HAUZ_GMAIL_CLIENT_ID=<CLIENT_ID> HAUZ_GMAIL_CLIENT_SECRET=<CLIENT_SECRET>
export "$(hauz gmail-auth)"        # sets HAUZ_GMAIL_REFRESH_TOKEN in this shell
```
1. Run the commands above. `hauz gmail-auth` writes a consent URL on stderr.
2. Open the URL in a browser on the same computer.
3. Approve the access.
4. Make sure that the command writes one line on stdout: `HAUZ_GMAIL_REFRESH_TOKEN=<token>`.

The browser goes to a temporary listener on `127.0.0.1:<random port>`. For all of the
steps, see [the gmail-auth sequence](../sequences/gmail-auth.md).

- **Exit 1:** the command gives exit 1 for these errors:
  - the client id or the secret is missing (stderr gives the name);
  - you do not approve the consent;
  - the `state` is not the same;
  - the token endpoint gives an error;
  - the response does not have a refresh token.
- **No timeout:** the command waits for the browser. To stop it, push Ctrl-C.
- **Cancel the token:** go to myaccount.google.com/permissions.

## Use the token
- `hauz fetch` lists the label. You can use `--after` and `--before` to give UTC days
  ([CLI reference](../reference/cli.md)).
- When `HAUZ_GMAIL_CLIENT_ID` is set, the server polls the label
  ([sequence](../sequences/fetch-poll.md)).

**First run:** fetch the same days two times. Make sure that the second run writes only
`Duplicate`. This result shows that Gmail gives the same raw bytes each time.
