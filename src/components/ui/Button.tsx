import { type ButtonHTMLAttributes, forwardRef } from "react";
import { Loader2 } from "lucide-react";

type Variant = "primary" | "secondary" | "ghost" | "danger";
type Size = "sm" | "md" | "lg";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
  loading?: boolean;
}

const sizeMap: Record<Size, string> = {
  sm: "h-[28px] px-3 text-xs gap-1.5 rounded-xl",
  md: "h-[32px] px-4 text-sm gap-2 rounded-xl",
  lg: "h-[36px] px-5 text-sm gap-2 rounded-[14px]",
};

const variantMap: Record<Variant, { base: string; style: React.CSSProperties }> = {
  primary: {
    base: "font-semibold active:scale-[0.97]",
    style: { background: "var(--cta-bg, var(--accent))", color: "var(--cta-text, #fff)", boxShadow: "inset 0 1px 0 rgba(255,255,255,0.16), 0 10px 22px rgba(0,0,0,0.10)" },
  },
  secondary: {
    base: "font-medium active:scale-[0.97]",
    style: { border: "1px solid var(--border-default)", color: "var(--text-secondary)", background: "var(--surface-control, var(--surface-card))" },
  },
  ghost: {
    base: "font-medium hover:bg-surface-hover active:scale-[0.97]",
    style: { color: "var(--text-secondary)" },
  },
  danger: {
    base: "font-medium active:scale-[0.97]",
    style: { border: "1px solid rgba(255,59,48,0.12)", color: "var(--error)", background: "rgba(255,59,48,0.10)" },
  },
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ variant = "secondary", size = "md", loading, children, className = "", disabled, style, ...props }, ref) => {
    const v = variantMap[variant];
    const s = sizeMap[size];

    return (
      <button
        ref={ref}
        disabled={disabled || loading}
        className={`inline-flex items-center justify-center transition-all duration-200 ease-out
          disabled:opacity-35 disabled:cursor-not-allowed ${s} ${v.base} ${className}`}
        style={{ ...v.style, ...style }}
        {...props}
      >
        {loading && <Loader2 size={13} className="animate-spin" />}
        {children}
      </button>
    );
  }
);

Button.displayName = "Button";
