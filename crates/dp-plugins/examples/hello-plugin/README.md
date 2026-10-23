# Hello Icon Crafter (example plugin)

This directory demonstrates the **op-scoped** plugin format used by
`dp-plugins`. It is intentionally *data only*: `plugin.json` declares identity,
permissions, a command, an event subscription and two settings.

## Why there is no code here

The sandbox does not execute plugin-authored JavaScript/WASM. The editor's host
registers a `PluginHandler` for a plugin id; whenever the plugin's command or an
event fires, the handler may only return typed `SandboxRequest`s such as
`GetDocumentSummary` or `ProposeInsert`. `dp-plugins` then:

1. checks the plugin is installed and enabled;
2. checks the request against the plugin's **declared and granted** permissions;
3. validates the arguments against the live document (unknown ids, non-container
   parents and size limits are rejected);
4. returns mutations as **proposals**, which the editor applies as normal
   undoable transactions after user confirmation;
5. isolates failures: a panicking or misbehaving plugin is disabled with an
   error while other plugins keep working.

Plugins therefore cannot read local files, open sockets, or mutate the document
behind the user's back by default. A future WASM sandbox can reuse the same
manifest + authorization surface.

## Installing

`PluginManifest::from_json(include_str!("plugin.json"))` plus
`PluginRegistry::install_json(...)` validates and installs it (disabled until
enabled/consented). The example manifest is covered by a unit test, so it always
stays loadable.
