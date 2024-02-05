// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import path from 'path';
import fs from "fs/promises";
import * as vscode from 'vscode';
import { LanguageClient, LanguageClientOptions, ServerOptions, State, TransportKind } from "vscode-languageclient/node";

// This method is called when your extension is activated
// Your extension is activated the very first time the command is executed
export function activate(context: vscode.ExtensionContext) {
	console.log('Congratulations, your extension "orchid-ls" is now active!');

	const LS_NAME = process.platform === "win32" ? "orchid-ls.exe" : "orchid-ls";
	const LS_PATH = context.asAbsolutePath(path.join("public", LS_NAME));
	const serverOptions: ServerOptions = {
		command: LS_PATH,
		transport: TransportKind.stdio,
	};
	const clientOptions: LanguageClientOptions = {
		documentSelector: [{ scheme: "file", language: "orchid" }],
	};
	const client = new LanguageClient(
		"OrchidLS",
		"Orchid Language Server",
		serverOptions,
		clientOptions
	);
	client.start().catch(e => console.error(e));
	context.subscriptions.push(client);

	// In debug mode, watch the server executable.
	if (context.extensionMode !== vscode.ExtensionMode.Production) {
		console.log("Watching ls exe");
		const watcher = vscode.workspace.createFileSystemWatcher(LS_PATH);
		context.subscriptions.push(watcher);
		watcher.onDidChange(() => client.restart());
	}

	// The command has been defined in the package.json file
	// Now provide the implementation of the command with registerCommand
	// The commandId parameter must match the command field in package.json
	let disposable = vscode.commands.registerCommand('orchid-ls.restart-server', async () => {
		if (client.state == State.Running) await client.restart();
		else if (client.state == State.Stopped) await client.start();
		else if (client.state == State.Starting) {
			await client.stop();
			await client.start();
		}
	});

	context.subscriptions.push(disposable);
}

// This method is called when your extension is deactivated
export function deactivate() {}