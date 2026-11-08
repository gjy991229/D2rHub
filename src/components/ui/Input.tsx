import { type InputHTMLAttributes, forwardRef, useId } from "react";

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  error?: string;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ label, error, className = "", id, style, "aria-describedby": describedBy, "aria-invalid": invalid, ...props }, ref) => {
    const generatedId = useId();
    const inputId = id || `input-${generatedId}`;
    const errorId = `${inputId}-error`;

    return (
      <div className="flex min-w-0 flex-col gap-1.5">
        {label && (
          <label
            htmlFor={inputId}
            className="text-xs font-medium text-text-secondary"
          >
            {label}
          </label>
        )}
        <input
          ref={ref}
          id={inputId}
          aria-invalid={error ? true : invalid}
          aria-describedby={[describedBy, error ? errorId : undefined].filter(Boolean).join(" ") || undefined}
          className={`ui-input w-full min-w-0 min-h-[36px] px-3 py-2 rounded-lg border
            text-sm placeholder:text-text-muted
            transition-[border-color,box-shadow] duration-150 ease-out
            focus:outline-none
            ${error ? "border-error/50" : "border-border-default hover:border-border-strong"}
            disabled:cursor-not-allowed disabled:opacity-50
            ${className}`}
          style={{ color: "var(--input-text, var(--text-primary))", backgroundColor: "var(--input-bg, var(--surface-card))", ...style }}
          {...props}
        />
        {error && (
          <p id={errorId} className="text-xs leading-relaxed text-error">{error}</p>
        )}
      </div>
    );
  }
);

Input.displayName = "Input";
