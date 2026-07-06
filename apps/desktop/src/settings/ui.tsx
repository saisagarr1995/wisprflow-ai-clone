import { ReactNode, useEffect, useRef, useState } from "react";

/** Page header: large title + optional subtitle, consistent across pages. */
export function PageHeader({ title, subtitle }: { title: string; subtitle?: string }) {
  return (
    <div className="mb-7">
      <h1 className="text-[22px] font-semibold tracking-tight text-white">{title}</h1>
      {subtitle && <p className="mt-1.5 text-[13px] leading-relaxed text-neutral-500">{subtitle}</p>}
    </div>
  );
}

export function Section({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="mb-7">
      {title && (
        <h2 className="mb-2.5 px-1 text-[11px] font-semibold uppercase tracking-widest text-neutral-500">
          {title}
        </h2>
      )}
      {/* No overflow-hidden: Select menus must be able to escape the card. */}
      <div className="divide-y divide-white/[0.06] rounded-2xl border border-white/[0.07] bg-white/[0.02]">
        {children}
      </div>
    </section>
  );
}

export function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 px-4 py-3.5">
      <div className="min-w-0">
        <div className="text-[13.5px] font-medium text-neutral-100">{label}</div>
        {hint && <div className="mt-0.5 text-[12px] leading-relaxed text-neutral-500">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function Toggle({ value, onChange }: { value: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      type="button"
      onClick={() => onChange(!value)}
      role="switch"
      aria-checked={value}
      className={`relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-200 ${
        value ? "bg-indigo-500" : "bg-white/15"
      }`}
    >
      <span
        className={`inline-block h-5 w-5 rounded-full bg-white shadow transition-transform duration-200 ${
          value ? "translate-x-[22px]" : "translate-x-0.5"
        }`}
      />
    </button>
  );
}

export type SelectOption = { value: string; label: string };

/**
 * Themed replacement for a native `<select>`. The OS popup of a native select
 * can't be styled and renders near-unreadable on the dark theme, so the menu
 * is our own: dark panel, hover highlight, check on the selected row, full
 * keyboard support (arrows / Enter / Escape), click-outside to close.
 */
export function Select({
  value,
  onChange,
  options,
  className = "w-48",
}: {
  value: string;
  onChange: (v: string) => void;
  options: SelectOption[];
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const rootRef = useRef<HTMLDivElement>(null);
  const selected = options.find((o) => o.value === value);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);

  const openMenu = () => {
    setActive(Math.max(0, options.findIndex((o) => o.value === value)));
    setOpen(true);
  };

  const pick = (v: string) => {
    onChange(v);
    setOpen(false);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (!open) {
      if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        openMenu();
      }
      return;
    }
    if (e.key === "Escape" || e.key === "Tab") {
      setOpen(false);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(options.length - 1, a + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(0, a - 1));
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (options[active]) pick(options[active].value);
    }
  };

  return (
    <div ref={rootRef} className={`relative ${className}`}>
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => (open ? setOpen(false) : openMenu())}
        onKeyDown={onKeyDown}
        className={`${selectCls} flex w-full items-center justify-between gap-2 text-left`}
      >
        <span className="truncate">{selected?.label ?? value}</span>
        <svg
          viewBox="0 0 12 12"
          className={`h-3 w-3 shrink-0 text-neutral-500 transition-transform duration-150 ${open ? "rotate-180" : ""}`}
          aria-hidden
        >
          <path
            d="M2.5 4.5L6 8l3.5-3.5"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.6"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>

      {open && (
        <div
          role="listbox"
          className="absolute left-0 z-50 mt-1.5 max-h-64 w-full min-w-max overflow-y-auto rounded-xl border border-white/10 bg-neutral-900 p-1 shadow-2xl shadow-black/60"
        >
          {options.map((o, i) => (
            <button
              key={o.value}
              type="button"
              role="option"
              aria-selected={o.value === value}
              onMouseEnter={() => setActive(i)}
              onClick={() => pick(o.value)}
              className={`flex w-full items-center justify-between gap-3 rounded-lg px-2.5 py-1.5 text-left text-[13px] transition-colors ${
                i === active ? "bg-white/10 text-white" : "text-neutral-200"
              }`}
            >
              <span className="truncate">{o.label}</span>
              {o.value === value && (
                <svg viewBox="0 0 16 16" className="h-3.5 w-3.5 shrink-0 text-indigo-400" aria-hidden>
                  <path
                    d="M3.5 8.5l3 3 6-6"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                </svg>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export const selectCls =
  "rounded-lg border border-white/10 bg-white/[0.04] px-3 py-1.5 text-[13px] text-neutral-100 outline-none transition-colors focus:border-indigo-500/70 hover:border-white/20";
export const inputCls =
  "rounded-lg border border-white/10 bg-white/[0.04] px-3 py-1.5 text-[13px] text-neutral-100 outline-none transition-colors placeholder:text-neutral-600 focus:border-indigo-500/70 hover:border-white/20 w-56";
export const btnCls =
  "rounded-lg bg-indigo-500 px-3.5 py-1.5 text-[13px] font-medium text-white transition-colors hover:bg-indigo-400 disabled:opacity-40 disabled:hover:bg-indigo-500";
export const btnGhostCls =
  "rounded-lg border border-white/10 px-3.5 py-1.5 text-[13px] font-medium text-neutral-300 transition-colors hover:bg-white/5 hover:text-white";
