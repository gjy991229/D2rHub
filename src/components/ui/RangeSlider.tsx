import { forwardRef, type CSSProperties, type InputHTMLAttributes } from "react";

interface RangeSliderProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "min" | "max" | "value"> {
  min: number;
  max: number;
  value: number;
}

type RangeSliderStyle = CSSProperties & {
  "--range-progress": string;
};

export const RangeSlider = forwardRef<HTMLInputElement, RangeSliderProps>(
  ({ min, max, value, className = "", style, ...props }, ref) => {
    const progress = max === min
      ? 0
      : Math.max(0, Math.min(100, ((value - min) / (max - min)) * 100));

    return (
      <input
        ref={ref}
        type="range"
        min={min}
        max={max}
        value={value}
        className={`settings-range-slider ${className}`}
        style={{
          "--range-progress": `${progress}%`,
          ...style,
        } as RangeSliderStyle}
        {...props}
      />
    );
  },
);

RangeSlider.displayName = "RangeSlider";
