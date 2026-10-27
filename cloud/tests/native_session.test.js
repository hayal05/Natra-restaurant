import test from "node:test";
import assert from "node:assert/strict";
import {endNativeSession} from "../../src/js/auth/session.js";

test("logout waits for native revocation before clearing the user and navigating",async()=>{
  const calls=[];let finish;
  const pending=endNativeSession({logout:()=>{calls.push("native");return new Promise(resolve=>{finish=resolve;});},
    clearUser:()=>calls.push("clear"),navigate:path=>calls.push(path)});
  await Promise.resolve();
  assert.deepEqual(calls,["native"]);
  finish();await pending;
  assert.deepEqual(calls,["native","clear","/login"]);
});
test("failed native logout never presents a completed logout",async()=>{
  const calls=[];
  await assert.rejects(()=>endNativeSession({logout:async()=>{throw new Error("IPC unavailable");},
    clearUser:()=>calls.push("clear"),navigate:path=>calls.push(path)}),/IPC unavailable/);
  assert.deepEqual(calls,[]);
});
