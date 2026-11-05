#!/usr/bin/env node
// Deterministic Claude Code (`claude --input-format stream-json
// --output-format stream-json --permission-prompt-tool stdio`), shaped like
// Claude Code 2.1.318. No network, no credentials, no model.
// Usage: node fake-claude-code.mjs <scenario> [claude args]
// Its argv and every message received are appended to $FAKE_LOG.
import { appendFileSync } from 'node:fs';
import { createInterface } from 'node:readline';

const scenario = process.argv[2] ?? 'normal';
const args = process.argv.slice(3);
const log = process.env.FAKE_LOG;
const flag = (name) => { const index = args.indexOf(name); return index >= 0 ? args[index + 1] : undefined; };
const sessionId = flag('--session-id') ?? flag('--resume') ?? 'none';
const mode = flag('--permission-mode') ?? 'default';
let hookId = null;
let nextRequest = 1;
let interrupted = false;
let turns = 0;
const waiting = new Map();

const send = (message) => process.stdout.write(`${JSON.stringify(message)}\n`);
const record = (message) => { if (log) appendFileSync(log, `${JSON.stringify(message)}\n`); };
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const ask = (request) => new Promise((resolve) => {
  const id = `cli-${nextRequest++}`;
  waiting.set(id, resolve);
  send({ type: 'control_request', request_id: id, request });
});
const assistant = (id, content, extra = {}) => send({ type: 'assistant', message: { id, role: 'assistant', content }, parent_tool_use_id: null, session_id: sessionId, ...extra });
const toolResult = (id, content, isError = false) => send({ type: 'user', message: { role: 'user', content: [{ type: 'tool_result', tool_use_id: id, content, is_error: isError }] }, parent_tool_use_id: null, session_id: sessionId });
const result = (subtype, extra = {}) => send({ type: 'result', subtype, is_error: subtype !== 'success', session_id: sessionId, ...extra });
const stream = (event) => send({ type: 'stream_event', event, parent_tool_use_id: null, session_id: sessionId });

record({ argv: args });
if (flag('--resume') && scenario === 'resume-missing') {
  result('error_during_execution', { errors: [`No conversation found with session ID: ${sessionId}`] });
  process.stderr.write(`No conversation found with session ID: ${sessionId}\n`);
  process.exit(1);
}

async function text(messageId, index, parts) {
  stream({ type: 'message_start', message: { id: messageId } });
  stream({ type: 'content_block_start', index, content_block: { type: 'text', text: '' } });
  for (const part of parts) stream({ type: 'content_block_delta', index, delta: { type: 'text_delta', text: part } });
  assistant(messageId, [{ type: 'text', text: parts.join('') }]);
  stream({ type: 'content_block_stop', index });
}

async function permission(toolUseId, toolName, input) {
  const answer = await ask({ subtype: 'can_use_tool', tool_name: toolName, input, tool_use_id: toolUseId, display_name: toolName, description: input.description, permission_suggestions: [{ type: 'addRules', rules: [{ toolName }], behavior: 'allow', destination: 'localSettings' }] });
  return answer.response;
}

async function turn(content) {
  turns += 1;
  interrupted = false;
  send({ type: 'system', subtype: 'init', session_id: sessionId, model: 'claude-fake', permissionMode: mode, cwd: process.cwd(), tools: [] });
  if (scenario === 'auth') {
    assistant('msg-auth', [{ type: 'text', text: 'Not logged in · Please run /login' }], { error: 'authentication_failed' });
    return result('success', { is_error: true, result: 'Not logged in · Please run /login' });
  }
  if (scenario === 'interrupt') {
    assistant('msg-long', [{ type: 'tool_use', id: 'toolu_long', name: 'Bash', input: { command: 'sleep 100' } }]);
    stream({ type: 'message_start', message: { id: 'msg-dots' } });
    stream({ type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } });
    while (!interrupted) {
      stream({ type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: '.' } });
      await sleep(30);
    }
    toolResult('toolu_long', "The user doesn't want to proceed with this tool use.", true);
    return result('error_during_execution', { errors: ['[ede_diagnostic] result_type=user last_content_type=n/a stop_reason=tool_use'], terminal_reason: 'aborted_tools' });
  }
  if (scenario === 'interrupt-approval') {
    assistant('msg-ask', [{ type: 'tool_use', id: 'toolu_ask', name: 'Bash', input: { command: 'rm -rf build' } }]);
    const answer = await permission('toolu_ask', 'Bash', { command: 'rm -rf build' });
    while (!interrupted) await sleep(20);
    toolResult('toolu_ask', answer?.message ?? 'stopped', true);
    return result('error_during_execution', { errors: [] });
  }
  if (scenario === 'permission') {
    const command = turns === 1 ? 'touch notes.md' : 'touch again.md';
    assistant(`msg-bash-${turns}`, [{ type: 'tool_use', id: `toolu_bash_${turns}`, name: 'Bash', input: { command, description: 'Create a note' } }]);
    const answer = await permission(`toolu_bash_${turns}`, 'Bash', { command, description: 'Create a note' });
    if (answer?.behavior === 'allow') toolResult(`toolu_bash_${turns}`, 'created');
    else toolResult(`toolu_bash_${turns}`, answer?.message ?? 'denied', true);
  }
  if (scenario === 'cancel-request') {
    assistant('msg-cancel', [{ type: 'tool_use', id: 'toolu_cancel', name: 'WebFetch', input: { url: 'https://example.com' } }]);
    const id = `cli-${nextRequest++}`;
    send({ type: 'control_request', request_id: id, request: { subtype: 'can_use_tool', tool_name: 'WebFetch', input: { url: 'https://example.com' }, tool_use_id: 'toolu_cancel' } });
    await sleep(300);
    send({ type: 'control_cancel_request', request_id: id });
    toolResult('toolu_cancel', 'cancelled by the CLI', true);
  }
  if (scenario === 'outside-write') {
    const outside = '/etc/cowiki-outside.md';
    const inside = `${process.cwd()}/inside.md`;
    for (const [id, path] of [['toolu_out', outside], ['toolu_in', inside]]) {
      assistant(`msg-${id}`, [{ type: 'tool_use', id, name: 'Write', input: { file_path: path, content: 'x' } }]);
      const answer = await ask({ subtype: 'hook_callback', callback_id: hookId, tool_use_id: id, input: { hook_event_name: 'PreToolUse', tool_name: 'Write', tool_input: { file_path: path, content: 'x' }, tool_use_id: id, session_id: sessionId, cwd: process.cwd() } });
      const denied = answer.response?.hookSpecificOutput?.permissionDecision === 'deny';
      toolResult(id, denied ? answer.response.hookSpecificOutput.permissionDecisionReason : 'written', denied);
    }
    // A permission prompt for a write outside the Space never becomes a card.
    assistant('msg-edit', [{ type: 'tool_use', id: 'toolu_edit', name: 'Edit', input: { file_path: '../outside.md', old_string: 'a', new_string: 'b' } }]);
    const edit = await permission('toolu_edit', 'Edit', { file_path: '../outside.md', old_string: 'a', new_string: 'b' });
    toolResult('toolu_edit', edit?.message ?? 'edited', edit?.behavior !== 'allow');
  }
  if (scenario === 'unknown-request') {
    await ask({ subtype: 'mcp_message', server_name: 'x', message: {} });
    send({ type: 'control_request', request_id: 'dialog-1', request: { subtype: 'request_user_dialog', dialog_kind: 'unknown', payload: {} } });
  }
  // A subagent's own messages carry parent_tool_use_id.
  send({ type: 'assistant', message: { id: 'msg-sub', role: 'assistant', content: [{ type: 'text', text: 'SUBAGENT' }] }, parent_tool_use_id: 'toolu_task', session_id: sessionId });
  stream({ type: 'message_start', message: { id: 'msg-think' } });
  stream({ type: 'content_block_start', index: 0, content_block: { type: 'thinking', thinking: '' } });
  stream({ type: 'content_block_delta', index: 0, delta: { type: 'thinking_delta', thinking: 'Considering' } });
  assistant('msg-think', [{ type: 'thinking', thinking: 'Considering' }]);
  stream({ type: 'content_block_stop', index: 0 });
  await text(`msg-${turns}`, 1, ['Hello ', `from Claude (${typeof content === 'string' ? content : 'blocks'})`]);
  assistant(`msg-read-${turns}`, [{ type: 'tool_use', id: `toolu_read_${turns}`, name: 'Read', input: { file_path: `${process.cwd()}/README.md` } }]);
  toolResult(`toolu_read_${turns}`, [{ type: 'text', text: '# Notes' }]);
  assistant(`msg-todo-${turns}`, [{ type: 'tool_use', id: `toolu_todo_${turns}`, name: 'TodoWrite', input: { todos: [{ content: 'Read the note', status: 'completed' }] } }]);
  toolResult(`toolu_todo_${turns}`, 'ok');
  send({ type: 'rate_limit_event', rate_limit_info: { status: 'allowed' } });
  result('success', { result: 'Hello from Claude' });
}

const lines = createInterface({ input: process.stdin });
lines.on('line', (line) => {
  let message;
  try { message = JSON.parse(line); } catch { return; }
  record(message);
  if (message.type === 'control_response') {
    const id = message.response?.request_id;
    waiting.get(id)?.(message.response);
    waiting.delete(id);
    return;
  }
  if (message.type === 'control_request') {
    const { request_id: id, request } = message;
    if (request.subtype === 'initialize') {
      hookId = request.hooks?.PreToolUse?.[0]?.hookCallbackIds?.[0] ?? null;
      return send({ type: 'control_response', response: { subtype: 'success', request_id: id, response: {
        current_permission_mode: scenario === 'bypass' ? 'bypassPermissions' : mode,
        claude_code_version: '2.1.318',
        account: { email: 'fake@example.com', subscriptionType: 'pro', apiProvider: 'firstParty' },
        models: [{ value: 'default' }],
      } } });
    }
    if (request.subtype === 'interrupt') {
      interrupted = true;
      return send({ type: 'control_response', response: { subtype: 'success', request_id: id, response: {} } });
    }
    return send({ type: 'control_response', response: { subtype: 'error', request_id: id, error: `fake does not implement ${request.subtype}` } });
  }
  if (message.type === 'user') {
    turn(message.message?.content);
  }
});
// Like Claude Code, closing stdin alone does not end a running turn.
lines.on('close', () => { if (scenario !== 'interrupt') process.exit(0); });
