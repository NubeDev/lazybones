// The MF-exposed mount module (`frontend.exposed-module = "./mount"`).
//
// The lazybones host imports this module after fetching the remote and calls its
// default export ONCE, handing it an `ExtSdkHandle`. We use the handle to register
// a top-level `route` (a nav entry + page); the host renders the page in the main
// content area and adds "Weather" to the sidebar.
import type { RemoteMount } from "@lazybones/ext-sdk";
import { CloudSun } from "lucide-react";
import { WeatherPage } from "./WeatherPage";

const mount: RemoteMount = (sdk) => {
  const unregister = sdk.register("route", {
    id: "weather",
    label: "Weather",
    icon: CloudSun,
    // `sdk` is captured so the page can reach the REST client and this
    // extension's own id (needed to address `POST /extensions/:id/invoke`).
    component: () => <WeatherPage sdk={sdk} />,
  });

  // Returning a cleanup lets the host tear the route down on disable/uninstall.
  return unregister;
};

export default mount;
