import { Moon, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/tooltip";
import { useTheme } from "@/lib/theme/theme-provider";

/** A single-button light/dark switch. */
export function ThemeToggle() {
  const { theme, toggle } = useTheme();
  const next = theme === "dark" ? "light" : "dark";
  return (
    <Tooltip label={`Switch to ${next} mode`}>
      <Button variant="ghost" size="icon-sm" onClick={toggle} aria-label="Toggle theme">
        {theme === "dark" ? <Moon /> : <Sun />}
      </Button>
    </Tooltip>
  );
}
