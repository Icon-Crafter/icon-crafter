/**
 * Plugin manager right sidebar.
 *
 * Uses only the engine's validated plugin commands: install (manifest or the
 * bundled example), remove, enable/disable, and typed settings. Nothing is
 * executed by the UI; manifests are validated by the engine.
 */
import { useState } from "react";

import type { SnapshotPlugin } from "../../engine/contract";
import { useAppStore, useCapabilities } from "../../engine/store";
import {
  confirmRemovePlugin,
  pluginInstall,
  pluginInstallExample,
  pluginSetEnabled,
  pluginSetSetting,
  togglePluginsPanel,
} from "../../engine/commands";
import { en } from "../../i18n/en";
import { Download, Puzzle, Trash2, X } from "../../icons";
import { Button } from "../primitives/IconButton";
import { NumberInput } from "../primitives/Fields";

function stringList(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.map((entry) => (typeof entry === "string" ? entry : JSON.stringify(entry)));
}

function manifestString(manifest: Record<string, unknown>, key: string): string | null {
  const value = manifest[key];
  return typeof value === "string" ? value : null;
}

function SettingEditor({
  plugin,
  settingKey,
  value,
  onSave,
}: {
  plugin: SnapshotPlugin;
  settingKey: string;
  value: unknown;
  onSave: (key: string, value: unknown) => void;
}) {
  const inputId = `plugin-${plugin.id}-${settingKey}`;

  if (typeof value === "boolean") {
    return (
      <label className="plugin-setting" htmlFor={inputId}>
        <input
          id={inputId}
          type="checkbox"
          className="plugin-checkbox"
          checked={value}
          onChange={(event) => onSave(settingKey, event.target.checked)}
        />
        <span className="plugin-setting-label">{settingKey}</span>
      </label>
    );
  }

  if (typeof value === "number") {
    return (
      <div className="plugin-setting">
        <span className="plugin-setting-label">{settingKey}</span>
        <NumberInput
          value={value}
          ariaLabel={settingKey}
          precision={3}
          onChange={() => undefined}
          onCommit={(next) => onSave(settingKey, next)}
        />
      </div>
    );
  }

  return (
    <div className="plugin-setting">
      <label className="plugin-setting-label" htmlFor={inputId}>
        {settingKey}
      </label>
      <div className="plugin-setting-row">
        <input
          id={inputId}
          type="text"
          className="text-input"
          defaultValue={typeof value === "string" ? value : JSON.stringify(value)}
          aria-label={settingKey}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            onSave(settingKey, event.currentTarget.value);
          }}
        />
        <Button
          variant="ghost"
          onClick={(event) => {
            const input = event.currentTarget
              .closest(".plugin-setting-row")
              ?.querySelector<HTMLInputElement>("input");
            if (input) onSave(settingKey, input.value);
          }}
        >
          {en.plugins.saveSetting}
        </Button>
      </div>
    </div>
  );
}

function PluginRow({ plugin }: { plugin: SnapshotPlugin }) {
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState(false);
  const settings = plugin.settings ?? {};
  const permissions = stringList(plugin.manifest.permissions);
  const commands = Array.isArray(plugin.manifest.commands) ? plugin.manifest.commands.length : 0;
  const panels = Array.isArray(plugin.manifest.panels) ? plugin.manifest.panels.length : 0;
  const description = plugin.description || manifestString(plugin.manifest, "description") || en.plugins.noDescription;
  const enabled = plugin.enabled && plugin.state === "enabled";

  const saveSetting = async (key: string, value: unknown) => {
    setBusy(true);
    await pluginSetSetting(plugin.id, key, value);
    setBusy(false);
  };

  return (
    <article className="plugin-card" aria-label={plugin.name}>
      <header className="plugin-card-header">
        <div className="plugin-card-heading">
          <h4 className="plugin-card-name">{plugin.name}</h4>
          <span className="plugin-version">v{plugin.version}</span>
        </div>
        <span
          className={
            plugin.state === "error"
              ? "plugin-badge plugin-badge--error"
              : enabled
                ? "plugin-badge plugin-badge--on"
                : "plugin-badge"
          }
        >
          {plugin.state === "error" ? en.plugins.stateError : enabled ? en.plugins.enabled : en.plugins.disabled}
        </span>
      </header>
      <p className="plugin-description">{description}</p>
      {plugin.last_error ? (
        <p className="form-error" role="alert">
          {en.plugins.lastError}: {plugin.last_error}
        </p>
      ) : null}
      <div className="plugin-card-actions">
        <Button
          variant="ghost"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            await pluginSetEnabled(plugin.id, !enabled);
            setBusy(false);
          }}
        >
          {enabled ? en.plugins.disable : en.plugins.enable}
        </Button>
        <Button variant="ghost" onClick={() => setExpanded(!expanded)} aria-expanded={expanded}>
          {expanded ? en.common.close : en.plugins.settingsTitle}
        </Button>
        <Button
          variant="ghost"
          icon={<Trash2 size={14} />}
          aria-label={en.plugins.remove}
          title={en.plugins.remove}
          onClick={() => confirmRemovePlugin(plugin.id, plugin.name)}
        >
          {en.plugins.remove}
        </Button>
      </div>

      {expanded ? (
        <div className="plugin-details">
          <div className="plugin-detail-block">
            <p className="plugin-detail-title">{en.plugins.permissions}</p>
            {permissions.length > 0 ? (
              <ul className="plugin-chips">
                {permissions.map((permission) => (
                  <li key={permission} className="plugin-chip">
                    {permission}
                  </li>
                ))}
              </ul>
            ) : (
              <p className="side-hint">{en.plugins.noPermissions}</p>
            )}
          </div>
          <div className="plugin-detail-block">
            <p className="plugin-detail-title">
              {en.plugins.commands} · {commands}
            </p>
            <p className="plugin-detail-title">
              {en.plugins.panels} · {panels}
            </p>
          </div>
          <div className="plugin-detail-block">
            <p className="plugin-detail-title">{en.plugins.settingsTitle}</p>
            {Object.keys(settings).length > 0 ? (
              Object.entries(settings).map(([key, value]) => (
                <SettingEditor
                  key={key}
                  plugin={plugin}
                  settingKey={key}
                  value={value}
                  onSave={(settingKey, next) => void saveSetting(settingKey, next)}
                />
              ))
            ) : (
              <p className="side-hint">{en.plugins.noSettings}</p>
            )}
          </div>
        </div>
      ) : null}
    </article>
  );
}

export function PluginsPanel() {
  const capabilities = useCapabilities();
  const doc = useAppStore((state) => state.engine.doc);
  const status = useAppStore((state) => state.status);
  const plugins = useAppStore((state) => state.engine.plugins) ?? [];
  const [manifestText, setManifestText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const enabled = capabilities.plugins === true && status === "ready" && doc !== null;

  const installManifest = async () => {
    setError(null);
    if (manifestText.trim() === "") {
      setError(en.plugins.manifestRequired);
      return;
    }
    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(manifestText) as Record<string, unknown>;
    } catch (parseError) {
      setError(`${en.plugins.validationTitle}: ${parseError instanceof Error ? parseError.message : String(parseError)}`);
      return;
    }
    setBusy(true);
    const result = await pluginInstall(parsed);
    setBusy(false);
    if (result.error) setError(`${en.plugins.installFailed} ${result.error}`);
    if (result.state) setManifestText("");
  };

  const installExample = async () => {
    setError(null);
    setBusy(true);
    const result = await pluginInstallExample();
    setBusy(false);
    if (result.error) setError(`${en.plugins.installFailed} ${result.error}`);
  };

  return (
    <section className="panel panel--right plugins-panel" aria-label={en.plugins.title}>
      <header className="side-panel-header">
        <div className="side-panel-heading">
          <p className="side-panel-kicker">
            <Puzzle size={12} aria-hidden="true" /> {en.plugins.label}
          </p>
          <h2 className="side-panel-title">{en.plugins.title}</h2>
          <p className="side-panel-subtitle">{en.plugins.subtitle}</p>
        </div>
        <button
          type="button"
          className="side-panel-close"
          aria-label={en.plugins.close}
          title={en.plugins.close}
          onClick={() => togglePluginsPanel()}
        >
          <X size={15} />
        </button>
      </header>

      <div className="side-panel-body">
        {!enabled ? (
          <div className="side-empty">
            <p className="side-empty-title">{en.plugins.disabledTitle}</p>
            <p className="side-empty-body">
              {capabilities.plugins !== true ? en.plugins.disabledHint : en.plugins.notConnected}
            </p>
          </div>
        ) : (
          <>
            <section className="side-section" aria-labelledby="plugins-install-title">
              <h3 className="side-section-title" id="plugins-install-title">
                {en.plugins.installTitle}
              </h3>
              <p className="side-hint">{en.plugins.installHint}</p>
              <textarea
                className="text-area text-area--code"
                rows={6}
                value={manifestText}
                aria-label={en.plugins.installTitle}
                placeholder={en.plugins.installPlaceholder}
                spellCheck={false}
                onChange={(event) => setManifestText(event.target.value)}
              />
              <div className="side-actions">
                <Button
                  variant="primary"
                  icon={<Puzzle size={14} />}
                  disabled={busy}
                  onClick={() => void installManifest()}
                >
                  {busy ? en.plugins.installing : en.plugins.install}
                </Button>
                <Button
                  variant="ghost"
                  icon={<Download size={14} />}
                  disabled={busy}
                  title={en.plugins.installExampleHint}
                  onClick={() => void installExample()}
                >
                  {en.plugins.installExample}
                </Button>
              </div>
              {error ? (
                <p className="form-error" role="alert">
                  {error}
                </p>
              ) : null}
              <p className="side-hint side-hint--muted">{en.plugins.trustNote}</p>
            </section>

            <section className="side-section" aria-labelledby="plugins-installed-title">
              <h3 className="side-section-title" id="plugins-installed-title">
                {en.plugins.installedTitle} · {plugins.length}
              </h3>
              {plugins.length > 0 ? (
                <div className="plugin-list">
                  {plugins.map((plugin) => (
                    <PluginRow key={plugin.id} plugin={plugin} />
                  ))}
                </div>
              ) : (
                <div className="side-empty side-empty--compact">
                  <p className="side-empty-title">{en.plugins.emptyTitle}</p>
                  <p className="side-empty-body">{en.plugins.emptyBody}</p>
                </div>
              )}
            </section>
          </>
        )}
      </div>
    </section>
  );
}
