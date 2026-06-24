// The "Weather" route page (registered into the `route` slot — a top-level nav
// entry). It is a THIN RENDERER: the WASM backend does 100% of the weather work.
// This page only
//
//   1. takes a place name,
//   2. calls  POST /extensions/:id/invoke { export: "weather", input: { location } }
//      via the SDK REST client — the daemon runs the WASM guest, which dials
//      Open-Meteo itself under the `http-fetch` allowlist, and
//   3. renders the typed result the guest returned.
//
// Everything goes through the SDK handle (`sdk.api`, `sdk.extensionId`); the page
// never fetches the weather API directly — that is the whole point of "backend
// does 100%".
import { useCallback, useState } from "react";
import type { ExtSdkHandle } from "@lazybones/ext-sdk";
import { CloudSun, Search, Wind, MapPin, AlertTriangle } from "lucide-react";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Input,
  Skeleton,
} from "./components/ui";

/** Mirror of the daemon `WeatherView` (the `weather` half of `InvokeResponse`). */
interface WeatherView {
  location: string;
  latitude: number;
  longitude: number;
  temperature_c: number;
  wind_kph: number;
  weather_code: number;
  description: string;
  observed_at: string;
  error?: string | null;
}

/** Mirror of the daemon `InvokeResponse` for the `weather` export. */
interface InvokeResponse {
  export: string;
  weather?: WeatherView | null;
  instantiation_micros: number | null;
  faulted: boolean;
  error?: string | null;
}

interface Props {
  sdk: ExtSdkHandle;
}

export function WeatherPage({ sdk }: Props) {
  const [location, setLocation] = useState("Berlin");
  const [result, setResult] = useState<WeatherView | null>(null);
  const [meta, setMeta] = useState<{ instantiation_micros: number | null } | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const lookup = useCallback(async () => {
    const place = location.trim();
    if (!place) return;
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      // `auth: true` attaches the loop bearer token the guarded invoke route
      // requires. The daemon compiles + runs the WASM guest under the
      // fuel/epoch/memory/timeout sandbox and the `http-fetch` allowlist.
      const res = await sdk.api.post<InvokeResponse>(
        `/extensions/${sdk.extensionId}/invoke`,
        { export: "weather", input: { location: place } },
        { auth: true },
      );
      if (res.faulted) {
        setError(res.error ?? "the extension faulted while fetching weather");
      } else if (res.weather?.error) {
        // A clean guest return that could not resolve the place.
        setError(res.weather.error);
      } else if (res.weather) {
        setResult(res.weather);
        setMeta({ instantiation_micros: res.instantiation_micros });
      } else {
        setError("no weather returned");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [sdk, location]);

  return (
    <div className="mx-auto max-w-2xl p-6">
      <div className="mb-1 flex items-center gap-2">
        <CloudSun className="size-6 text-accent" />
        <h1 className="text-lg font-semibold tracking-tight">Weather</h1>
        <Badge variant="accent" className="ml-2">
          backend · WASM
        </Badge>
      </div>
      <p className="mb-5 text-xs text-muted-foreground">
        Current conditions fetched <strong>by the WASM extension itself</strong>{" "}
        (Open-Meteo, no API key) — this page only renders what the guest returns.
      </p>

      <form
        className="mb-5 flex items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void lookup();
        }}
      >
        <Input
          value={location}
          onChange={(e) => setLocation(e.target.value)}
          placeholder="Enter a city, e.g. Tokyo"
          aria-label="Place name"
          data-testid="weather-input"
        />
        <Button type="submit" disabled={loading} data-testid="weather-search">
          <Search />
          {loading ? "Fetching…" : "Get weather"}
        </Button>
      </form>

      {error && (
        <Card className="border-status-blocked/30">
          <CardContent className="flex items-center gap-2 p-4 text-sm text-status-blocked">
            <AlertTriangle className="size-4" />
            {error}
          </CardContent>
        </Card>
      )}

      {loading && !error && (
        <Card>
          <CardHeader>
            <Skeleton className="h-5 w-40" />
          </CardHeader>
          <CardContent className="flex gap-6">
            <Skeleton className="h-12 w-24" />
            <Skeleton className="h-12 w-24" />
          </CardContent>
        </Card>
      )}

      {result && !loading && !error && (
        <Card data-testid="weather-result">
          <CardHeader>
            <CardTitle className="flex items-center gap-1.5">
              <MapPin className="size-4 text-muted-foreground" />
              {result.location}
            </CardTitle>
            <span className="text-xs text-muted-foreground">
              {result.latitude.toFixed(2)}, {result.longitude.toFixed(2)} · observed{" "}
              {result.observed_at}
            </span>
          </CardHeader>
          <CardContent className="flex flex-wrap items-end gap-8">
            <div>
              <div
                className="text-4xl font-semibold tabular-nums"
                data-testid="weather-temp"
              >
                {result.temperature_c.toFixed(1)}°C
              </div>
              <div className="mt-1 text-sm text-muted-foreground">
                {result.description}
              </div>
            </div>
            <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
              <Wind className="size-4" />
              {result.wind_kph.toFixed(1)} km/h
            </div>
            <Badge variant="accent">WMO {result.weather_code}</Badge>
          </CardContent>
        </Card>
      )}

      {meta && result && !loading && (
        <p className="mt-3 text-[11px] text-muted-foreground/70">
          guest cold instantiation{" "}
          {meta.instantiation_micros != null
            ? `${meta.instantiation_micros}µs`
            : "n/a"}{" "}
          · fetched over wasi:http under the http-fetch allowlist
        </p>
      )}
    </div>
  );
}
