/* @refresh reload */
import { render } from "solid-js/web";
import { Router, Route } from "@solidjs/router";
import App from "./App";
import Timeline from "./pages/Timeline";
import Search from "./pages/Search";
import Chat from "./pages/Chat";
import Summaries from "./pages/Summaries";
import Entities from "./pages/Entities";
import Settings from "./pages/Settings";
import "./index.css";

const root = document.getElementById("root");

if (root) {
  render(
    () => (
      <Router root={App}>
        <Route path="/" component={Timeline} />
        <Route path="/search" component={Search} />
        <Route path="/chat" component={Chat} />
        <Route path="/summaries" component={Summaries} />
        <Route path="/entities" component={Entities} />
        <Route path="/settings" component={Settings} />
      </Router>
    ),
    root
  );
}
