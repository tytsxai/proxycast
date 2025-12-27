import React, { useMemo } from "react";
import { cn } from "@/lib/utils";
import { providerIcons, providerTypeToIcon } from "./utils";

interface ProviderIconProps {
  providerType: string;
  size?: number | string;
  className?: string;
  showFallback?: boolean;
}

export const ProviderIcon: React.FC<ProviderIconProps> = ({
  providerType,
  size = 24,
  className,
  showFallback = true,
}) => {
  const iconName = providerTypeToIcon[providerType] || providerType;
  const iconSvg = useMemo(() => {
    if (!Object.prototype.hasOwnProperty.call(providerIcons, iconName)) {
      return undefined;
    }

    const svg = providerIcons[iconName];
    const s = svg.trimStart();

    if (!s.toLowerCase().startsWith("<svg")) {
      return undefined;
    }
    if (/<\s*script\b/i.test(s)) {
      return undefined;
    }
    if (/<\s*foreignobject\b/i.test(s)) {
      return undefined;
    }
    if (/\son[a-z]+\s*=/i.test(s)) {
      return undefined;
    }
    if (/javascript\s*:/i.test(s)) {
      return undefined;
    }

    return svg;
  }, [iconName]);

  const sizeStyle = useMemo(() => {
    const sizeValue = typeof size === "number" ? `${size}px` : size;
    return {
      width: sizeValue,
      height: sizeValue,
      fontSize: sizeValue,
      lineHeight: 1,
    };
  }, [size]);

  if (iconSvg) {
    return (
      <span
        className={cn(
          "inline-flex items-center justify-center flex-shrink-0",
          className,
        )}
        style={sizeStyle}
        dangerouslySetInnerHTML={{ __html: iconSvg }}
      />
    );
  }

  // Fallback：显示首字母
  if (showFallback) {
    const initials = providerType
      .split("_")
      .map((word) => word[0])
      .join("")
      .toUpperCase()
      .slice(0, 2);
    const fallbackFontSize =
      typeof size === "number" ? `${Math.max(size * 0.5, 12)}px` : "0.5em";
    return (
      <span
        className={cn(
          "inline-flex items-center justify-center flex-shrink-0 rounded-lg",
          "bg-muted text-muted-foreground font-semibold",
          className,
        )}
        style={sizeStyle}
      >
        <span style={{ fontSize: fallbackFontSize }}>{initials}</span>
      </span>
    );
  }

  return null;
};
