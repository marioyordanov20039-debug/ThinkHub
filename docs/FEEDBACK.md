# Feedback by email

The Feedback tab lets a user choose Bug report, Feature idea or General feedback, enter a
subject/message and review an email addressed to **info.thinkhub@protonmail.com**.
**Open email app** uses the desktop's standard mailto handler. The user checks the draft and
presses Send in that email app; ThinkHub does not send mail itself or claim delivery.
No hosted feedback server, embedded SMTP credentials or email account login is required in ThinkHub.

The app version is included. Model, Linux distribution and kernel are included only if the
user selects the checkbox, which is off by default. No raw logs, serial numbers, settings files
or other diagnostics are attached. The sender address comes from the user's email account.
Messages stay in memory across tab navigation and refreshes; closing ThinkHub discards the draft.

Subject limit: 120 characters. Message limit: 1,000 characters; longer reports can continue in
the email composer. Empty, oversized or invalid drafts show a validation message. Subject/body
are encoded separately using RFC 6068 mailto semantics so special characters cannot add
recipients or headers. Review content is plain text.

If there is no working mailto handler, use **Copy email** in the review window and paste it into
an email composer, or configure the desktop's default email application. Opening a browser or
composer does not prove a message was sent. Confirm a test message in the destination mailbox
before treating delivery as verified.

## Developer validation (no hardware changes)

Unit tests cover recipient/URI encoding, Unicode, bounds/header injection and optional context
allowlisting. An explicit native smoke mode fills the real GTK fields, switches tabs to check
draft retention, opens the review through its normal button and captures only ThinkHub's windows:

```sh
THINKHUB_FEEDBACK_TEST_DIR=/absolute/local/test-directory gui/target/release/thinkhub
```

Use this without another ThinkHub instance. It writes a test email/URI and screenshots only to
that directory; no email app is opened by default. Additionally setting
`THINKHUB_FEEDBACK_TEST_OPEN_MAIL=1` triggers **Open email app**, never Send. For automated handoff
checks, use isolated XDG directories with a test mailto handler that records the supplied URI.
Do not confuse that check with delivery to a real mailbox. Never send CI test messages to the
public feedback address. Ordinary eight-page capture mode does not open a draft or send mail.

## Delivery verification

On 2026-10-24 the generated test draft was opened in Gmail and sent to the configured
feedback address; the maintainer confirmed receipt before publishing the update. Local tests
also verified the exact GIO handoff and draft retention. No mail-client credentials are shipped
in the app. Other email clients still depend on their own mailto support and account setup.
