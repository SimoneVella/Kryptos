import { useEffect, useState } from "react";
import { Icon } from "../components/Icon";
import { api, type DuplicateGroup, type EntrySummary, type TitleSuggestion, type TrashedEntry } from "../lib/api";
import { Avatar, Sheet, useToast } from "../components/ui";
import { useT } from "../i18n";

/** Readable titles for entries named after a host ("account.live.com" -> "Microsoft"). */
export function TitlesSheet({ items, onClose, onDone }: { items: TitleSuggestion[]; onClose: () => void; onDone: () => void }) {
  const { t, errorMessage } = useT();
  const toast = useToast();
  const [picked, setPicked] = useState(() => new Set(items.map((i) => i.id)));
  const apply = async () => {
    try {
      const n = await api.applyTitles(items.filter((i) => picked.has(i.id)).map((i) => ({ id: i.id, title: i.suggested })));
      toast(t("cleanup.titlesDone", { n }));
      onDone();
    } catch (e) {
      toast(errorMessage(e));
    }
  };
  return (
    <Sheet onClose={onClose} title={t("cleanup.titlesTitle")}>
      <p className="muted" style={{ margin: "6px 0 14px" }}>{t("cleanup.titlesIntro")}</p>
      <div className="detail-list">
        {items.map((i) => (
          <label key={i.id} className="detail-item check-row">
            <input
              type="checkbox"
              checked={picked.has(i.id)}
              onChange={(e) => {
                const next = new Set(picked);
                if (e.target.checked) next.add(i.id);
                else next.delete(i.id);
                setPicked(next);
              }}
            />
            <span className="row-main">
              <span className="row-sub mono">{i.title}</span>
              <span className="row-title">{i.suggested}</span>
            </span>
          </label>
        ))}
      </div>
      <div className="form-actions" style={{ marginTop: 16 }}>
        <button className="btn btn-secondary" onClick={onClose}>{t("common.cancel")}</button>
        <button className="btn btn-primary" disabled={!picked.size} onClick={apply}>{t("cleanup.titlesApply", { n: picked.size })}</button>
      </div>
    </Sheet>
  );
}

/** Groups of the same site and account: merging keeps the newest, the rest go to the trash. */
export function DuplicatesSheet({
  groups,
  byId,
  onClose,
  onDone,
}: {
  groups: DuplicateGroup[];
  byId: Map<string, EntrySummary>;
  onClose: () => void;
  onDone: () => void;
}) {
  const { t, errorMessage } = useT();
  const toast = useToast();
  const [left, setLeft] = useState(groups);
  const merge = async (list: DuplicateGroup[]) => {
    try {
      for (const g of list) await api.mergeDuplicates(g.ids);
      toast(t("cleanup.mergedToast", { n: list.length }));
      const rest = left.filter((g) => !list.includes(g));
      setLeft(rest);
      onDone();
      if (!rest.length) onClose();
    } catch (e) {
      toast(errorMessage(e));
    }
  };
  const exact = left.filter((g) => g.same_password);
  return (
    <Sheet onClose={onClose} title={t("cleanup.dupTitle")}>
      <p className="muted" style={{ margin: "6px 0 14px" }}>{t("cleanup.dupIntro")}</p>
      {exact.length > 0 && (
        <button className="btn btn-primary btn-block" style={{ marginBottom: 14 }} onClick={() => merge(exact)}>
          <Icon name="check" size={18} /> {t("cleanup.mergeExact", { n: exact.length })}
        </button>
      )}
      <div className="detail-list">
        {left.map((g) => {
          const first = byId.get(g.ids[0]);
          return (
            <div key={g.ids.join()} className="detail-item">
              <Avatar name={g.title} urls={first?.urls} />
              <span className="row-main">
                <span className="row-title">{g.title}</span>
                <span className="row-sub">
                  {g.username || "—"} · {t(g.same_password ? "cleanup.copiesSame" : "cleanup.copiesDifferent", { n: g.ids.length })}
                </span>
              </span>
              <button className="btn btn-secondary btn-sm" onClick={() => merge([g])}>{t("cleanup.merge")}</button>
            </div>
          );
        })}
      </div>
    </Sheet>
  );
}

/** Entries deleted in the last 30 days: restore or delete for good. */
export function TrashSheet({ onClose, onChanged }: { onClose: () => void; onChanged: () => void }) {
  const { t, formatDate, errorMessage } = useT();
  const toast = useToast();
  const [items, setItems] = useState<TrashedEntry[] | null>(null);
  const load = () => api.trashList().then(setItems);
  useEffect(() => {
    load();
  }, []);
  const act = async (fn: () => Promise<void>, msg: string) => {
    try {
      await fn();
      toast(msg);
      await load();
      onChanged();
    } catch (e) {
      toast(errorMessage(e));
    }
  };
  return (
    <Sheet onClose={onClose} title={t("trash.title")}>
      <p className="muted" style={{ margin: "6px 0 14px" }}>{t("trash.intro")}</p>
      {items?.length === 0 ? (
        <div className="empty">
          <div className="empty-icon"><Icon name="delete" size={28} /></div>
          <p className="muted">{t("trash.empty")}</p>
        </div>
      ) : (
        <>
          <div className="detail-list">
            {items?.map((e) => (
              <div key={e.id} className="detail-item">
                <Avatar name={e.title} urls={e.urls} />
                <span className="row-main">
                  <span className="row-title">{e.title}</span>
                  <span className="row-sub">{t("trash.deletedOn", { date: formatDate(e.deleted_at) })}</span>
                </span>
                <button className="btn btn-secondary btn-sm" onClick={() => act(() => api.trashRestore(e.id), t("trash.restored", { title: e.title }))}>
                  {t("trash.restore")}
                </button>
                <button className="icon-btn" title={t("trash.purge")} onClick={() => act(() => api.trashPurge(e.id), t("trash.purged"))}>
                  <Icon name="close" size={18} />
                </button>
              </div>
            ))}
          </div>
          {!!items?.length && (
            <button className="btn btn-danger btn-block" style={{ marginTop: 14 }} onClick={() => act(api.trashEmpty, t("trash.emptied"))}>
              <Icon name="delete" size={18} /> {t("trash.emptyAll")}
            </button>
          )}
        </>
      )}
    </Sheet>
  );
}
