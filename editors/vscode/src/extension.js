const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

let client;
let lifecycle = Promise.resolve();

function runLifecycle(action) {
  // Serialize activation, restart, and shutdown, including a failed startup.
  const pending = lifecycle.then(action, action);
  lifecycle = pending;
  return pending;
}

async function startLanguageServer() {
  const serverPath = vscode.workspace.getConfiguration('nacre').get('serverPath', 'nacre');
  const serverOptions = {
    command: serverPath,
    args: ['lsp']
  };
  const clientOptions = {
    documentSelector: [{ scheme: 'file', language: 'nacre' }]
  };

  client = new LanguageClient('nacre', 'Nacre Language Server', serverOptions, clientOptions);
  await client.start();
}

async function stopLanguageServer() {
  if (client?.isRunning()) {
    await client.stop();
  }
  client = undefined;
}

function restartLanguageServer() {
  return runLifecycle(async () => {
    await stopLanguageServer();
    await startLanguageServer();
  });
}

function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand('nacre.restartLanguageServer', restartLanguageServer)
  );
  return runLifecycle(startLanguageServer);
}

function deactivate() {
  return runLifecycle(stopLanguageServer);
}

module.exports = { activate, deactivate };
