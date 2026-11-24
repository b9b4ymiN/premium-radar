//! A sortable column header — the scanner table's `th-sort` look, generic so
//! any client-side-sorted table can reuse it. First click sorts descending,
//! the next ascending; leaving the table's own order is a separate reset.

export interface SortSpec<K extends string> {
  key: K;
  dir: 1 | -1;
}

export function SortHeader<K extends string>({ label, sortKey, sort, onSort, className, title }: {
  label: string;
  sortKey: K;
  sort: SortSpec<K> | null;
  onSort: (key: K) => void;
  className?: string;
  title?: string;
}) {
  const active = sort?.key === sortKey ? sort : null;
  return (
    <th className={className} aria-sort={active ? (active.dir === 1 ? "ascending" : "descending") : "none"}>
      <button
        type="button"
        className="th-sort"
        onClick={() => onSort(sortKey)}
        title={title ?? (active ? "เรียงอยู่ — คลิกเพื่อสลับทิศทาง" : "เรียงตามคอลัมน์นี้")}
      >
        {label}
        <i className={active ? "on" : ""}>{active ? (active.dir === 1 ? "▲" : "▼") : "⇅"}</i>
      </button>
    </th>
  );
}
