import { ReactNode } from "react";

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mb-8">
      <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-neutral-500">
        {title}
      </h2>
      <div className="rounded-xl border border-neutral-800 bg-neutral-900/60 divide-y divide-neutral-800">
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
    <div className="flex items-center justify-between gap-6 px-4 py-3">
      <div className="min-w-0">
        <div className="text-sm font-medium text-neutral-200">{label}</div>
        {hint && <div className="mt-0.5 text-xs text-neutral-500">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function Toggle({ value, onChange }: { value: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      onClick={() => onChange(!value)}
      className={`h-6 w-11 rounded-full p-0.5 transition-colors ${
        value ? "bg-emerald-600" : "bg-neutral-700"
      }`}
      role="switch"
      aria-checked={value}
    >
      <div
        className={`h-5 w-5 rounded-full bg-white transition-transform ${
          value ? "translate-x-5" : ""
        }`}
      />
    </button>
  );
}

export const selectCls =
  "rounded-lg border border-neutral-700 bg-neutral-800 px-3 py-1.5 text-sm text-neutral-100 outline-none focus:border-emerald-600";
export const inputCls = selectCls + " w-56";
export const btnCls =
  "rounded-lg bg-emerald-700 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-600 disabled:opacity-40";
