import { Sidebar } from "@/components/layout/sidebar";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ThemeProvider } from "@/lib/theme/theme-provider";
import { QueryProvider } from "./query-provider";
import { useRouter, ViewRenderer } from "./router";

/** The root shell: providers + the sidebar/content split. */
export function App() {
  return (
    <ThemeProvider>
      <QueryProvider>
        <TooltipProvider delayDuration={300}>
          <Shell />
        </TooltipProvider>
      </QueryProvider>
    </ThemeProvider>
  );
}

function Shell() {
  const { view, navigate } = useRouter();
  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground">
      <Sidebar view={view} onNavigate={navigate} />
      <main className="min-w-0 flex-1">
        <ViewRenderer view={view} onNavigate={navigate} />
      </main>
    </div>
  );
}
