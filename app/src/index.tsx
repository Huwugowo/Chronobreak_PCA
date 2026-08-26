/* @refresh reload */
import { render } from "solid-js/web";
import App from "./App";
import { initializeReplayBenchmark } from "./benchmark";
import "./styles/global.css";

void initializeReplayBenchmark();
render(() => <App />, document.getElementById("root") as HTMLElement);
