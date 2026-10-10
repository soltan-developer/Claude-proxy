# Claude Proxy 2.0.10

Claude Proxy is a Windows desktop app that connects Claude Desktop and Claude CLI directly to OpenAI-compatible model providers. Provider credentials, URLs, and model routes start empty on a fresh install.

For developer maintenance and build instructions, see [`DEVELOPMENT.md`](DEVELOPMENT.md).

## Run the portable app

Run the version-numbered `Claude-Proxy-v2.0.10-Portable.exe`. Each new user-facing build is assigned a unique version, and its version appears in the sidebar and title area. The interface is bundled inside the app; you do not need to start a development server or connect to `localhost:1420`. Windows WebView2 Runtime must be installed. The orange tray icon means the proxy is stopped; green means it is running. Closing the window sends the app to the system tray. To exit, right-click the tray icon and choose **Quit**.

The top-bar language control switches the interface between English and Persian. The selected language is remembered on this computer.

## Setup

1. Enter your OpenAI-compatible base URL, with or without `/v1`, then choose the authentication method and enter your API key.
2. Fetch the provider's model list. If the provider does not expose `/v1/models`, enter model IDs manually.
3. Map one or more Claude roles, such as Fable or Sonnet, to your provider's model IDs.
4. Open **Claude clients** and apply the settings for Claude Desktop or Claude CLI with that client's button.

The default local port is `18181`; port `10808` is not used. You can change the port on the **Claude clients** page. If Claude was already configured, apply its settings again after changing the port. At runtime, the app listens only on `127.0.0.1`.

## Long conversations and context limits

If a provider reports a context overflow, **Automatically trim older turns** retries the same request while removing the oldest complete turns. It preserves the current message, the system prompt, and the output budget; trimming continues until the request fits or only the current turn remains, so long conversations do not stop at an arbitrary turn count. When older turns are dropped, the freed context room is handed back to the model as output budget. The same recovery applies to the automatic continuation rounds used for long answers. Claude's displayed conversation history is not deleted or rewritten, but the model will not receive the trimmed turns in that request. Keep a short summary in a new message when you need to continue an older topic accurately. This option is enabled by default and can be turned off in settings.

## Token usage

Open **Token usage** to review saved usage by day or month, across all models or grouped by model. The date controls include 7-day and 30-day ranges, the current month, and a custom start and end date. Choose how often the page refreshes automatically, or refresh it manually. The history is saved locally and remains available after restarting the app.

Usage is marked as **actual** when the provider returns token counts. If a provider omits counts, the app estimates them from the request and generated text; estimated input and output totals are identified in the usage view. For streaming requests, the app asks compatible providers to include usage metadata and retries without that optional field when a provider rejects it. Token history stores dates, model names, counts, and estimate flags; it does not store conversation text or credentials.

## Diagnostics

Open **Diagnostics** to review recent proxy, provider, configuration, and usage-history problems. Filter the list to warnings and errors, refresh it, or copy/download a report for the developer. Clearing diagnostics removes the local log history. Diagnostic reports do not contain prompts, completions, provider response bodies, or API keys.

## Security and configuration

- The provider API key and gateway token are protected with the current Windows account's DPAPI.
- Claude client settings are written to Claude's own configuration files. The local token is readable there because the client needs to send it with requests.
- The app creates a backup before editing Claude's files and preserves other existing settings.
- The app's configuration is stored in the current Windows user's application data folder.
- Token usage and diagnostics are stored locally in `usage-history.jsonl` and `diagnostics.jsonl` in the app data folder. The app keeps a bounded history and does not upload these files.

## License

Claude Proxy is distributed under the custom **Salmanzadeh Non-Commercial License 1.0** (`LicenseRef-Salmanzadeh-NonCommercial-1.0`). Personal, academic, educational, research, and other non-commercial use is free. Commercial use requires prior written permission from the developer. See [`LICENSE.txt`](LICENSE.txt) for the full terms. This is a source-available, non-commercial license, not an OSI-approved open-source license. Third-party components retain their own licenses.

Developer: Salmanzadeh — [Eitaa channel](https://eitaa.com/salmanzadeh_ir)
