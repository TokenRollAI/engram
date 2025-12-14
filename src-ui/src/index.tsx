/* @refresh reload */
import { render } from "solid-js/web";
import { Router, Route } from "@solidjs/router";
import App from "./App";
import Timeline from "./pages/Timeline";
import Search from "./pages/Search";
import Settings from "./pages/Settings";
import "./index.css";

const root = document.getElementById("root");

if (root) {
  render(
    () => (
      <Router root={App}>
        <Route path="/" component={Timeline} />
        <Route path="/search" component={Search} />
        <Route path="/settings" component={Settings} />
      </Router>
    ),
    root
  );
}
