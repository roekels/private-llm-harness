# Changelog

The release script uses the section for each version as its release notes. Keep them short and for users.

## 0.8.0

- **Updates from the app.** About shows when a new version is out. Select **Install update**: the app downloads it, checks its signature, replaces itself and restarts. Your sessions, settings and sandbox stay.
- Checks for updates at start and once a day. Turn it off in About. Nothing installs without your click.
- First version on GitHub releases. Install this one by hand once; later versions arrive in the app.
- About links to the source code.

## 0.7.1

- Anti-block guard: at most 20 new connections per second to one site from the sandbox, on by default.

## 0.7.0

- Compaction at 50% of the context window, with a session memory you can read and edit.
- 1Password support removed. Keys live in the Keychain.

## 0.6.x

- `.dmg` install, Setup assistant (verified install of Apple container, sandbox build, isolation check).
- Browser DevTools: network, console, html and eval.

## 0.5.0

- Browser tool in the sandbox VM, browser pane, automatic titles, skills, About view.
