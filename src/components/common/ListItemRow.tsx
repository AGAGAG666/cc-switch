import React from "react";
import { cn } from "@/lib/utils";

interface ListItemRowProps {
  isLast?: boolean;
  children: React.ReactNode;
  className?: string;
}

export const ListItemRow: React.FC<ListItemRowProps> = ({
  isLast,
  children,
  className,
}) => {
  return (
    <div
      className={cn(
        "group flex items-center gap-3 px-4 py-2.5 hover:bg-subtle transition-colors",
        !isLast && "border-b border-border",
        className,
      )}
    >
      {children}
    </div>
  );
};
