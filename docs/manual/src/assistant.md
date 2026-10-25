# AI assistant

The assistant is a chat panel that can operate EZ-YX-craft for you. It uses the same commands as the
buttons, so everything it does appears in the project and can be undone.

<!-- feature:assistant.chat -->
## Ask the assistant

Ask in plain language — "make a 200×100 walnut sign that says Workshop" — and the assistant builds
features, plans toolpaths and (only after you confirm) writes files or moves the machine. Every change
is undoable.

**Steps**

1. Click **🤖 Assistant** in the top bar.
2. Open its settings and choose **Claude** or **OpenAI** (with an API key, or the `ANTHROPIC_API_KEY` /
   `OPENAI_API_KEY` environment variable) or **Ollama** (a free local model).
3. Type a request and press `Ctrl+Enter`.

> **Warning:** The assistant always asks before it runs anything that moves the machine or writes a
> file. Read the request before you click **Allow**.

*Available since version 0.2.0.*
<!-- /feature:assistant.chat -->
