# Changelog

The release script uses the section for each version as its release notes. Keep them short and for users.

## 0.8.1

- **Replies render as Markdown:** headings, lists, tables, quotes, links, bold, italic and code. Still text only: no HTML from the model ever runs. Links open in your browser.
- **Approve until done:** a third button on approval cards. It approves this step and every later one until the model's final answer. The next message asks again.
- **Audit export:** the **Audit** button in the footer saves the whole session (system prompt, memory, messages, reasoning, every tool call and its output) as one CSV file in Downloads.
- **Tool rounds setting:** choose how many tool rounds run before the model pauses (Settings > Tools, default 16). A **Continue** button picks up where it stopped.
- **System prompt and skills in Settings:** read the system prompt, read every skill, and add, edit or delete your own. New sessions list your skills; the model fetches one when it needs it.

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
