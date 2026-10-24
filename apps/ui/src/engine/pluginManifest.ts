/**
 * Plugin manifest parsing and projection helpers.
 *
 * The engine is the authority on validation; this module only turns the
 * pasted JSON into a value and presents the parts of an installed manifest the
 * manager shows. It never executes plugin code.
 */

export interface ManifestParseResult {
  manifest: Record<string, unknown> | null;
  error: string | null;
}

/** Parses a JSON object pasted into the install field. */
export function parsePluginManifest(text: string): ManifestParseResult {
  const trimmed = text.trim();
  if (trimmed === "") return { manifest: null, error: "empty" };
  let value: unknown;
  try {
    value = JSON.parse(trimmed);
  } catch (error) {
    return { manifest: null, error: error instanceof Error ? error.message : String(error) };
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return { manifest: null, error: "manifest must be a JSON object" };
  }
  return { manifest: value as Record<string, unknown>, error: null };
}

/** String list from a manifest field (non-string entries are JSON encoded). */
export function manifestStringList(manifest: Record<string, unknown>, key: string): string[] {
  const value = manifest[key];
  if (!Array.isArray(value)) return [];
  return value.map((entry) => (typeof entry === "string" ? entry : JSON.stringify(entry)));
}

/** Count of entries in an array-valued manifest field. */
export function manifestEntryCount(manifest: Record<string, unknown>, key: string): number {
  const value = manifest[key];
  return Array.isArray(value) ? value.length : 0;
}

/** First non-empty display name from the manifest. */
export function manifestDisplayName(manifest: Record<string, unknown>, fallback: string): string {
  for (const key of ["name", "display_name", "title", "id"]) {
    const value = manifest[key];
    if (typeof value === "string" && value.trim() !== "") return value.trim();
  }
  return fallback;
}
