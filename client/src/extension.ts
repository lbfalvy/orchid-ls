// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import path from 'path';
import child_process from "child_process";
import fs from "fs/promises";
import fscb from "fs";
import * as vscode from 'vscode';
import { LanguageClient, LanguageClientOptions, ServerOptions, State, StateChangeEvent, Trace, TransportKind } from "vscode-languageclient/node";

// This method is called when your extension is activated
// Your extension is activated the very first time the command is executed
export function activate(context: vscode.ExtensionContext) {
	console.log('Congratulations, your extension "orchidls" is now active!');
	const OUTLINE_FLOWER = context.asAbsolutePath(path.join("public", "outline-flower.svg"))
	const FILL_FLOWER = context.asAbsolutePath(path.join("public", "fill-flower.svg"))
	const LS_NAME = process.platform === "win32" ? "orchid-ls.exe" : "orchid-ls";
	const LS_PATH = context.asAbsolutePath(path.join("public", LS_NAME));
	const GRAMMAR_PATH = context.asAbsolutePath(path.join("public", "orchid.tmLanguage.json"));
	const clientOptions: LanguageClientOptions = {
		documentSelector: [{ scheme: "file", language: "orchid" }],
	};
	const channel = vscode.window.createOutputChannel("orchidls");
	const client = new LanguageClient(
		"OrchidLS",
		"Orchid Language Server",
		() => {
			const proc = child_process.exec(LS_PATH);
			proc.stderr?.on("data", ev => channel.append(ev));
			return Promise.resolve(proc);
		},
		clientOptions
	);
	client.setTrace(Trace.Verbose).catch(console.error);
	const statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 1);
	statusBarItem.show();
	statusBarItem.command = "orchidls.restart-server";
	client.start().catch(console.error);
	context.subscriptions.push(client);
	context.subscriptions.push(client.onDidChangeState(handleState));
	function decor(color: string|vscode.ThemeColor): vscode.TextEditorDecorationType {
		return vscode.window.createTextEditorDecorationType({
			color
		})
	}
	const decorations = new Map([
		["variable", decor(new vscode.ThemeColor("syntax.variable.orchid"))],
		["function", decor(new vscode.ThemeColor("syntax.constant.orchid"))],
		["parameter", decor(new vscode.ThemeColor("syntax.parameter.orchid"))],
		["macro", decor(new vscode.ThemeColor("syntax.keyword.orchid"))],
	])
	context.subscriptions.push(client.onNotification("client/syntacticTokens", data => {
		console.log("Received syntactic tokens:", data);
		const groups: [string, vscode.Range[]][] = data.legend.map((name: string) => [name, []]);
		console.log(data.tokens.length, "tokens received");
		const glossary = new Set();
		for (const [line, char, len, type] of data.tokens) {
			const [name, ranges] = groups[type];
			glossary.add(name);
			ranges.push(new vscode.Range(
				new vscode.Position(line, char),
				new vscode.Position(line, char + len)
			));
		}
		console.log(`Found token types:`, glossary);
		for (const ed of vscode.window.visibleTextEditors) {
			if (ed.document.uri.toString() !== data.textDocument.uri) continue;
			for (const [name, ranges] of groups) {
				const decor = decorations.get(name);
				if (!decor) {
					console.log(`Missing decorations for ${name}`);
					continue;
				}
				console.log(`Token type ${name} found ${ranges.length} times`);
				ed.setDecorations(decor, ranges);
			}
		}	
	}));
	async function awaitClientNotStarting(): Promise<State> {
		while(true) {
			if (client.state !== State.Starting) return client.state;
			let sub: vscode.Disposable|undefined;
			await new Promise(r => sub = client.onDidChangeState(r, null));
			sub?.dispose();
		}
	}
	async function restartIfReady() {
		if (client.state == State.Running) await client.restart();
		else if (client.state == State.Stopped) await client.start();
	}
	async function restartWhenReady() {
		await awaitClientNotStarting();
		await restartIfReady();
	}
	function handleState() {
		if (client.state == State.Stopped) {
			statusBarItem.text = "$(error) OrchidLS stopped"
		} else if (client.state == State.Starting) {
			statusBarItem.text = `$(loading~spin) OrchidLS`;
		} else {
			statusBarItem.text = "$(check) OrchidLS";
		}
	}
	function updateTrace() {
		context.globalState
		const tracestr = vscode.workspace.getConfiguration().get("orchidls.trace.server", "verbose");
		const trace = Trace.fromString(tracestr);
		console.log(`Orchid LS trace is ${trace}`);
		if (trace !== undefined) client.setTrace(trace).catch(console.error);
	}
	handleState();
	// In debug mode, watch the server executable.
	if (context.extensionMode !== vscode.ExtensionMode.Production) {
		console.log("Watching ls exe");
		const ls_watcher = fscb.watch(LS_PATH);
		ls_watcher.on("change", async () => {
			console.log("Reloading ls exe")
			await restartWhenReady().catch(() => {});
			await restartWhenReady().catch(console.error);
		});
		const grammar_watcher = fscb.watch(GRAMMAR_PATH);
		grammar_watcher.on("change", () => {
			vscode.commands.executeCommand("workbench.action.reloadWindow");
		});
		context.subscriptions.push({ dispose: () => {
			ls_watcher.close();
			grammar_watcher.close();
	 }});
	}
	updateTrace();
	context.subscriptions.push(vscode.workspace.onDidChangeConfiguration(updateTrace));
	// The command has been defined in the package.json file
	// Now provide the implementation of the command with registerCommand
	// The commandId parameter must match the command field in package.json
	context.subscriptions.push(vscode.commands.registerCommand('orchidls.restart-server', async () => {
		await restartWhenReady().catch(console.error);
		channel.show();
	}));
}

// This method is called when your extension is deactivated
export function deactivate() {}
