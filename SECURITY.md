# Security

Please **do not open a public issue** for a security problem. Use GitHub's private reporting instead: **Security → Report a vulnerability** on this repository.

You will get an answer within 7 days. Once fixed, the issue is described in the release notes, with credit if you wish.

## What matters most here

- Anything that could leak an API key, a Claude sign-in, a licence key or the user's code.
- Anything that lets the agent run an irreversible action (push, deploy, delete) the user did not allow.
- A way to make the app accept a forged licence token or remote flag.
- An update that is not signed by the project's update key.
