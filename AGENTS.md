# Repository boundary

This repository owns CoWiki client surfaces:

- browser UI and shared frontend components
- the Tauri desktop client and local-only engine
- client-side Cloud API/Git contracts, sync flows, CLI, and Agent skills

All Cloud service implementation belongs in the sibling `cowiki-backend`
repository. Do not add server API handlers, authentication services, database
schemas or migrations, Git hosting services, backend containers, or deployment
configuration here.

When a client feature requires a backend change, implement and test the service
contract in `cowiki-backend`, then consume that contract from this repository.
