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

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ variant = "secondary", size = "md", loading, children, className = "", disabled, type = "button", ...props }, ref) => {
    const s = sizeMap[size];

    return (
      <button
        ref={ref}
        type={type}
        disabled={disabled || loading}
        aria-busy={loading || undefined}
        className={`ui-button ui-button--${variant} inline-flex items-center justify-center whitespace-nowrap transition-[background-color,border-color,color,opacity] duration-150 ease-out
          focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2
          disabled:opacity-50 disabled:cursor-not-allowed ${s} ${className}`}
        {...props}
      >
        {loading && <Loader2 size={14} className="animate-spin shrink-0" aria-hidden="true" />}
        {children}
      </button>
    );
  }
);

Button.displayName = "Button";
