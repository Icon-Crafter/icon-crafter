import { useAppStore } from "../../engine/store";
import { en } from "../../i18n/en";
import { format } from "../../i18n/en";
import { Contrast, Palette, Sparkles, Type } from "../../icons";
import { EmptyState } from "../primitives/Layout";

interface StyleRow {
  id: string;
  name: string;
}

function StyleList({
  title,
  items,
  icon,
  emptyHint,
}: {
  title: string;
  items: StyleRow[];
  icon: React.ReactNode;
  emptyHint: string;
}) {
  const pushToast = useAppStore((state) => state.pushToast);
  if (items.length === 0) return null;
  return (
    <div className="panel-section">
      <div className="panel-section-header">
        <span className="panel-section-title">
          {icon}
          {title}
        </span>
        <span className="panel-tab-count">{items.length}</span>
      </div>
      <div className="asset-list">
        {items.map((item) => (
          <button
            key={item.id}
            type="button"
            className="asset-row"
            title={en.assets.footer}
            onClick={() =>
              pushToast({
                kind: "info",
                title: format(en.toasts.unsupported, { command: item.name }),
                message: emptyHint,
              })
            }
          >
            <span className={`asset-swatch${title === en.assets.textStyles ? " asset-swatch--text" : ""}`}>
              {title === en.assets.textStyles ? <Type size={12} /> : title === en.assets.effects ? <Sparkles size={12} /> : null}
            </span>
            <span className="asset-name">{item.name}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

/** Local styles exposed by the document summary. Applying them is not wired yet. */
export function AssetsPanel() {
  const summary = useAppStore((state) => state.engine.summary);
  const styles = summary?.styles;

  const total =
    (styles?.paints.length ?? 0) + (styles?.texts.length ?? 0) + (styles?.effects.length ?? 0);

  if (!styles || total === 0) {
    return (
      <div className="panel-section" style={{ flex: "1 1 auto" }}>
        <EmptyState icon={<Palette size={18} />} title={en.assets.emptyTitle} body={en.assets.emptyBody} />
        <div className="panel-footer panel-footer--hint">{en.assets.footer}</div>
      </div>
    );
  }

  return (
    <div className="panel-section scroll-thin" style={{ flex: "1 1 auto", overflowY: "auto" }}>
      <StyleList
        title={en.assets.colors}
        items={[...styles.paints]}
        icon={<Palette size={12} />}
        emptyHint={en.assets.applyHint}
      />
      <StyleList
        title={en.assets.textStyles}
        items={[...styles.texts]}
        icon={<Type size={12} />}
        emptyHint={en.assets.applyHint}
      />
      <StyleList
        title={en.assets.effects}
        items={[...styles.effects]}
        icon={<Contrast size={12} />}
        emptyHint={en.assets.applyHint}
      />
      <div className="panel-footer panel-footer--hint">{en.assets.footer}</div>
    </div>
  );
}
