import { ReactNode } from "react";

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
      <div className="divide-y divide-white/[0.06] overflow-hidden rounded-2xl border border-white/[0.07] bg-white/[0.02]">
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
      onClick={() => onChange(!value)}
      className={`relative h-6 w-11 rounded-full transition-colors duration-200 ${
        value ? "bg-indigo-500" : "bg-white/12"
      }`}
      role="switch"
      aria-checked={value}
    >
      <span
        className={`absolute top-0.5 h-5 w-5 rounded-full bg-white shadow-sm transition-transform duration-200 ${
          value ? "translate-x-[22px]" : "translate-x-0.5"
        }`}
      />
    </button>
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
