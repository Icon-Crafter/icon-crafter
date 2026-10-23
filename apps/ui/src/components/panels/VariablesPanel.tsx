import { useState } from "react";
import type { VariableCollectionEntry, VariableEntry, VariableValueJson } from "../../engine/contract";
import {
  activeModeId,
  describeVariableValue,
  findVariable,
  modeName,
  parseValueInput,
  valueForMode,
  valueToInput,
  variableTypeLabel,
} from "../../engine/designSystem";
import { useAppStore } from "../../engine/store";
import {
  createColorVariable,
  createFloatVariable,
  createVariableCollection,
  setActiveVariableMode,
  setVariableValue,
} from "../../engine/commands";
import { en } from "../../i18n/en";
import { colorToHex } from "../../utils/color";
import { Plus, Trash2, Variable } from "../../icons";

type CreateKind = "color" | "float" | null;

/** Resolve a variable value to a hex swatch, following alias chains. */
function resolveSwatch(
  collections: readonly VariableCollectionEntry[],
  value: VariableValueJson | null,
  depth = 0,
): string | null {
  if (!value || depth > 8) return null;
  if (value.type === "color") return colorToHex(value.color).toUpperCase();
  if (value.type === "alias") {
    const found = findVariable(collections, value.variable_id);
    if (!found) return null;
    return resolveSwatch(collections, valueForMode(found.variable, found.collection), depth + 1);
  }
  return null;
}

interface CollectionBlockProps {
  collection: VariableCollectionEntry;
  collections: readonly VariableCollectionEntry[];
}

function CollectionBlock({ collection, collections }: CollectionBlockProps) {
  const [createKind, setCreateKind] = useState<CreateKind>(null);
  const [name, setName] = useState("");
  const [colorValue, setColorValue] = useState("#4FD1C5");
  const [floatValue, setFloatValue] = useState("8");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  const variables = collection.variables ?? [];
  const active = activeModeId(collection);
  const lookupName = (id: string) => findVariable(collections, id)?.variable.name ?? null;

  const commitCreate = () => {
    if (createKind === "color") {
      void createColorVariable(collection.id, name, colorValue);
    } else if (createKind === "float") {
      const parsed = Number(floatValue);
      if (Number.isFinite(parsed)) void createFloatVariable(collection.id, name, parsed);
    }
    setCreateKind(null);
    setName("");
  };

  const commitValue = (variable: VariableEntry) => {
    const value = parseValueInput(variable.resolved_type, draft);
    if (value) void setVariableValue(variable.id, value, active);
    setEditingId(null);
  };

  return (
    <div className="variable-collection">
      <div className="variable-collection-head">
        <span className="variable-collection-name">{collection.name}</span>
        <span className="variable-collection-count">
          {en.variables.variablesCount.replace("{count}", String(variables.length))}
        </span>
      </div>
      <div className="variable-modes" role="group" aria-label={en.variables.modes}>
        {collection.modes.map((mode) => (
          <button
            key={mode.id}
            type="button"
            className={`mode-pill${mode.id === active ? " mode-pill--active" : ""}`}
            aria-pressed={mode.id === active}
            title={en.variables.switchMode.replace("{mode}", modeName(collection, mode.id))}
            onClick={() => void setActiveVariableMode(collection.id, mode.id)}
          >
            {modeName(collection, mode.id)}
          </button>
        ))}
      </div>

      {variables.length === 0 && <p className="asset-group-hint">{en.variables.noVariables}</p>}

      <div className="variable-list">
        {variables.map((variable) => {
          const value = valueForMode(variable, collection);
          const display = describeVariableValue(value, lookupName);
          const swatch =
            variable.resolved_type === "color"
              ? resolveSwatch(collections, value)
              : null;
          const editing = editingId === variable.id;
          return (
            <div key={variable.id} className="variable-row">
              <span
                className="variable-swatch"
                style={{ background: swatch ?? "transparent" }}
                aria-hidden="true"
              />
              <span className="variable-name" title={variable.name}>
                {variable.name}
              </span>
              <span className="variable-type">{variableTypeLabel(variable.resolved_type)}</span>
              {editing ? (
                <input
                  className="text-input variable-value-input"
                  autoFocus
                  value={draft}
                  aria-label={en.variables.editValue.replace("{mode}", modeName(collection, active))}
                  onChange={(event) => setDraft(event.target.value)}
                  onBlur={() => commitValue(variable)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") commitValue(variable);
                    if (event.key === "Escape") setEditingId(null);
                  }}
                />
              ) : (
                <button
                  type="button"
                  className={`variable-value${display.kind === "alias" ? " variable-value--alias" : ""}`}
                  title={en.variables.editValue.replace("{mode}", modeName(collection, active))}
                  disabled={display.kind === "alias"}
                  onClick={() => {
                    setEditingId(variable.id);
                    setDraft(valueToInput(value));
                  }}
                >
                  {display.text}
                </button>
              )}
            </div>
          );
        })}
      </div>

      {createKind === null ? (
        <div className="variable-create-actions">
          <button
            type="button"
            className="button button--ghost button--small"
            onClick={() => setCreateKind("color")}
          >
            <Plus size={12} /> {en.variables.addColor}
          </button>
          <button
            type="button"
            className="button button--ghost button--small"
            onClick={() => setCreateKind("float")}
          >
            <Plus size={12} /> {en.variables.addFloat}
          </button>
        </div>
      ) : (
        <div className="variable-create-form">
          <input
            className="text-input"
            autoFocus
            placeholder={en.variables.variableName}
            aria-label={en.variables.variableName}
            value={name}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") commitCreate();
              if (event.key === "Escape") setCreateKind(null);
            }}
          />
          {createKind === "color" ? (
            <input
              className="text-input variable-color-input"
              aria-label={en.variables.colorValue}
              value={colorValue}
              onChange={(event) => setColorValue(event.target.value)}
            />
          ) : (
            <input
              className="text-input variable-color-input"
              aria-label={en.variables.floatValue}
              value={floatValue}
              onChange={(event) => setFloatValue(event.target.value)}
            />
          )}
          <button type="button" className="button button--primary button--small" onClick={commitCreate}>
            {en.common.add}
          </button>
          <button
            type="button"
            className="icon-button"
            aria-label={en.common.cancel}
            onClick={() => setCreateKind(null)}
          >
            <Trash2 size={12} />
          </button>
        </div>
      )}
    </div>
  );
}

/** Variables tab: collections, modes and per-mode values. */
export function VariablesPanel() {
  const summary = useAppStore((state) => state.engine.summary);
  const collections = summary?.variables ?? [];
  const [creating, setCreating] = useState(false);
  const [collectionName, setCollectionName] = useState("");

  const submitCollection = () => {
    const name = collectionName.trim();
    if (!name) return;
    void createVariableCollection(name);
    setCollectionName("");
    setCreating(false);
  };

  return (
    <div className="variables-panel">
      <div className="panel-section-header">
        <span className="panel-section-title">{en.variables.title}</span>
        <button
          type="button"
          className="icon-button"
          aria-label={en.variables.addCollection}
          title={en.variables.addCollection}
          onClick={() => setCreating((open) => !open)}
        >
          <Plus size={14} />
        </button>
      </div>

      {creating && (
        <div className="variable-create-form">
          <input
            className="text-input"
            autoFocus
            placeholder={en.variables.collectionName}
            aria-label={en.variables.collectionName}
            value={collectionName}
            onChange={(event) => setCollectionName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") submitCollection();
              if (event.key === "Escape") setCreating(false);
            }}
          />
          <button type="button" className="button button--primary button--small" onClick={submitCollection}>
            {en.common.add}
          </button>
        </div>
      )}

      {collections.length === 0 ? (
        <div className="empty-state">
          <span className="empty-state-icon">
            <Variable size={18} />
          </span>
          <p className="empty-state-title">{en.variables.emptyTitle}</p>
          <p className="empty-state-body">{en.variables.emptyBody}</p>
        </div>
      ) : (
        <div className="variable-collections">
          {collections.map((collection) => (
            <CollectionBlock key={collection.id} collection={collection} collections={collections} />
          ))}
        </div>
      )}

      <p className="panel-footer-hint">{en.variables.createHint}</p>
    </div>
  );
}
