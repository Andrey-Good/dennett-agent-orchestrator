import React from "react";
import { projectChat } from "./lib/commands";

export function App(): React.JSX.Element {
  const [result, setResult] = React.useState("Implementation skeleton ready");
  return <main style={{fontFamily:"system-ui",padding:24}}>
    <h1>Dennett Desktop Skeleton</h1>
    <p>{result}</p>
    <button onClick={async()=>setResult(await projectChat("hello"))}>Run thin vertical slice</button>
  </main>;
}
