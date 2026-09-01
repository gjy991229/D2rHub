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
  sm: "min-h-[32px] px-3 text-xs gap-1.5 rounded-lg",
  md: "min-h-[36px] px-4 text-sm gap-2 rounded-lg",
  lg: "min-h-[40px] px-5 text-sm gap-2 rounded-[10px]",
};

const variantMap: Record<Variant, { base: string; style: React.CSSProperties }> = {
  primary: {
    base: "font-semibold hover:brightness-105 active:translate-y-px",
    style: { border: "1px solid transparent", background: "var(--cta-bg, var(--accent))", color: "var(--cta-text, #fff)" },
  },
  secondary: {
    base: "font-medium hover:bg-surface-hover active:translate-y-px",
    style: { border: "1px solid var(--border-default)", color: "var(--text-secondary)", background: "transparent" },
  },
  ghost: {
    base: "font-medium hover:bg-surface-hover active:translate-y-px",
    style: { border: "1px solid transparent", color: "var(--text-secondary)" },
  },
  danger: {
    base: "font-medium hover:brightness-105 active:translate-y-px",
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
        className={`inline-flex items-center justify-center whitespace-nowrap transition-[background-color,border-color,color,filter,transform] duration-150 ease-out
          focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2
          disabled:translate-y-0 disabled:opacity-50 disabled:cursor-not-allowed disabled:brightness-100 ${s} ${v.base} ${className}`}
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
