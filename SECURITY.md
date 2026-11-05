# Security

This agent is not an OS sandbox. File tools enforce canonical workspace paths, but concurrent filesystem changes (TOCTOU), hard links, and hostile workspace files remain risks. Use an isolated account or container for untrusted projects.

Shell is disabled by default. `original --allow-shell` authorizes arbitrary model-generated commands with your user permissions, including network and filesystem access outside the workspace. Windows timeout attempts to terminate the process tree; Unix termination currently covers the immediate shell only. Detached descendants may survive.

API credentials saved by `/login` are plaintext in the user configuration directory. Prefer ORIGINAL_API_KEY and trusted HTTPS endpoints. Login input is currently visible; do not screen-share while entering secrets. Conversation history is saved locally. Workspace `.env` files are not automatically loaded.

Report vulnerabilities privately to the repository maintainer; do not post keys or exploit data in public issues. This review is not a penetration test or a dependency security certification.
