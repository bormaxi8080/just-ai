import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "../src/App";
import * as api from "../src/api";

const events = vi.hoisted(() => ({ listener: (_event: any) => {} }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (_name, listener) => {
  events.listener = listener;
  return () => {};
}) }));
vi.mock("../src/api", async (original) => ({
  ...await original<typeof api>(),
  inspectProject: vi.fn(), recentRuns: vi.fn(), prepareRun: vi.fn(),
  executeRun: vi.fn(), cancelRun: vi.fn(),
}));

const project = { warnings: [], recipes: [{
  name: "hello", namepath: "hello", doc: "Safe greeting", body: ["echo hello"],
  parameters: [{ name: "message", kind: "singular", default: "hello" }],
  dependencies: [], private: false, risk: "low" as const, risks: [],
}] };
const prepared = { request: { project_root: ".", recipe: "hello", arguments: ["hello"] },
  preview: ["echo hello"], risk: "low" as const, findings: [], policy: { decision: "allow" as const } };
const result = { success: true, exit_code: 0, stdout: "hello\n", stderr: "" };

beforeEach(() => {
  vi.mocked(api.inspectProject).mockReset().mockResolvedValue(project);
  vi.mocked(api.recentRuns).mockReset().mockResolvedValue([]);
  vi.mocked(api.prepareRun).mockReset().mockResolvedValue(prepared);
  vi.mocked(api.executeRun).mockReset().mockResolvedValue(result);
  vi.mocked(api.cancelRun).mockReset().mockResolvedValue(true);
});
afterEach(cleanup);
async function open() {
  const user = userEvent.setup(); render(<App />);
  await screen.findByRole("button", { name: "Prepare & run" });
  return user;
}

describe("desktop user workflows", () => {
  it("preserves literal argv and refreshes persisted history", async () => {
    const user = await open();
    const argument = "a; echo secret & spaces";
    vi.mocked(api.recentRuns).mockResolvedValue([{ id: "1", recipe: "hello", arguments: [argument],
      started_at_ms: 1, duration_ms: 2, success: true, cancelled: false, exit_code: 0,
      stdout_tail: "saved output", stderr_tail: "" }]);
    await user.clear(screen.getByLabelText(/message/));
    await user.type(screen.getByLabelText(/message/), argument);
    await user.click(screen.getByRole("button", { name: "Prepare & run" }));
    await screen.findByText("saved output");
    expect(api.prepareRun).toHaveBeenCalledWith({ project_root: ".", recipe: "hello", arguments: [argument] });
    expect(api.executeRun).toHaveBeenCalledWith(prepared, { confirmation: "none" });
  });

  it("refuses denied runs before execute IPC", async () => {
    vi.mocked(api.prepareRun).mockResolvedValue({ ...prepared, policy: { decision: "deny", reason: "Project policy denies this recipe" } });
    const user = await open();
    await user.click(screen.getByRole("button", { name: "Prepare & run" }));
    await screen.findByText(/Project policy denies/);
    expect(api.executeRun).not.toHaveBeenCalled();
  });

  it("rejects an incorrect typed phrase and transports the correct phrase", async () => {
    const policy = { decision: "confirm_typed" as const, phrase: "DEPLOY EXACTLY" };
    vi.mocked(api.prepareRun).mockResolvedValue({ ...prepared, policy });
    const prompt = vi.spyOn(window, "prompt").mockReturnValue("wrong");
    const user = await open();
    await user.click(screen.getByRole("button", { name: "Prepare & run" }));
    await screen.findByText(/Confirmation phrase does not match/);
    expect(api.executeRun).not.toHaveBeenCalled();
    prompt.mockReturnValue(policy.phrase);
    await user.click(screen.getByRole("button", { name: "Prepare & run" }));
    await waitFor(() => expect(api.executeRun).toHaveBeenCalledWith({ ...prepared, policy }, { confirmation: "typed", phrase: policy.phrase }));
  });

  it("cancels confirmation without executing", async () => {
    vi.mocked(api.prepareRun).mockResolvedValue({ ...prepared, policy: { decision: "confirm" } });
    vi.spyOn(window, "confirm").mockReturnValue(false);
    const user = await open();
    await user.click(screen.getByRole("button", { name: "Prepare & run" }));
    expect(api.executeRun).not.toHaveBeenCalled();
  });

  it("clears old recipes when the root changes or inspection fails", async () => {
    const user = await open();
    await user.clear(screen.getByLabelText("Project root"));
    await user.type(screen.getByLabelText("Project root"), "/missing/project");
    expect(screen.queryByRole("button", { name: "Prepare & run" })).toBeNull();
    vi.mocked(api.inspectProject).mockRejectedValue(new Error("Cannot inspect missing project"));
    await user.click(screen.getByRole("button", { name: "Inspect project" }));
    await screen.findByText(/Cannot inspect missing/);
    expect(screen.queryByRole("button", { name: "Prepare & run" })).toBeNull();
  });

  it("ignores a stale inspection response after changing roots", async () => {
    let resolve!: (value: api.ProjectContext) => void;
    vi.mocked(api.inspectProject).mockReturnValue(new Promise((done) => { resolve = done; }));
    const user = userEvent.setup(); render(<App />);
    await user.type(screen.getByLabelText("Project root"), "/other");
    await act(async () => { resolve(project); });
    expect(screen.queryByRole("button", { name: "Prepare & run" })).toBeNull();
  });

  it("streams output, disables root changes and permits cancellation", async () => {
    let finish!: (value: api.RunResult) => void;
    vi.mocked(api.executeRun).mockReturnValue(new Promise((done) => { finish = done; }));
    const user = await open();
    await user.click(screen.getByRole("button", { name: "Prepare & run" }));
    const cancel = await screen.findByRole("button", { name: "Cancel run" });
    expect((screen.getByLabelText("Project root") as HTMLInputElement).disabled).toBe(true);
    act(() => events.listener({ payload: { event: "stdout", text: "live progress" } }));
    await screen.findByText("live progress");
    await user.click(cancel);
    expect(api.cancelRun).toHaveBeenCalledOnce();
    await act(async () => finish(result));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Cancel run" })).toBeNull());
  });
});
