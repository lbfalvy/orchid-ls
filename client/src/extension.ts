// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import path from 'path';
import childp from "child_process";
import fs from "fs";
import * as vsc from 'vscode';
import * as lsp  from "vscode-languageclient/node";

// This method is called when your extension is activated
// Your extension is activated the very first time the command is executed
export function activate(context: vsc.ExtensionContext) {
	console.log('Congratulations, your extension "orchidls" is now active!');
	const OUTLINE_FLOWER = context.asAbsolutePath(path.join("public", "outline-flower.svg"))
	const FILL_FLOWER = context.asAbsolutePath(path.join("public", "fill-flower.svg"))
	const LS_NAME = process.platform === "win32" ? "orchid-ls.exe" : "orchid-ls";
	const LS_PATH = context.asAbsolutePath(path.join("public", LS_NAME));
	const GRAMMAR_PATH = context.asAbsolutePath(path.join("public", "orchid.tmLanguage.json"));
	const clientOptions: lsp.LanguageClientOptions = {
		documentSelector: [{ scheme: "file", language: "orchid" }],
	};
	const client = new lsp.LanguageClient(
		"OrchidLS",
		"Orchid Language Server",
		() => {
			const proc = childp.exec(LS_PATH);
			return Promise.resolve(proc);
		},
		clientOptions
	);
	client.setTrace(lsp.Trace.Verbose).catch(console.error);
	const statusBarItem = vsc.window.createStatusBarItem(vsc.StatusBarAlignment.Left, 1);
	statusBarItem.show();
	statusBarItem.command = "orchidls.restart-server";
	client.start().catch(console.error);
	context.subscriptions.push(client);
	context.subscriptions.push(client.onDidChangeState(handleState));
	function decor(color: string|vsc.ThemeColor): vsc.TextEditorDecorationType {
		return vsc.window.createTextEditorDecorationType({
			color
		})
	}
	const decorations = new Map([
		["variable", decor(new vsc.ThemeColor("syntax.variable.orchid"))],
		["function", decor(new vsc.ThemeColor("syntax.constant.orchid"))],
		["parameter", decor(new vsc.ThemeColor("syntax.parameter.orchid"))],
		["keyword", decor(new vsc.ThemeColor("syntax.keyword.orchid"))],
	])
	context.subscriptions.push(client.onNotification("client/syntacticTokens", data => {
		console.log("Received syntactic tokens:", data);
		const groups: [string, vsc.Range[]][] = data.legend.map((name: string) => [name, []]);
		console.log(data.tokens.length, "tokens received");
		const glossary = new Set();
		for (const [line, char, len, type] of data.tokens) {
			const [name, ranges] = groups[type];
			glossary.add(name);
			ranges.push(new vsc.Range(
				new vsc.Position(line, char),
				new vsc.Position(line, char + len)
			));
		}
		console.log(`Found token types:`, glossary);
		for (const ed of vsc.window.visibleTextEditors) {
			if (ed.document.uri.toString() !== data.textDocument.uri) continue;
			for (const [name, ranges] of groups) {
				const decor = decorations.get(name);
				if (!decor) {
					if (0 < ranges.length) console.log(`Missing decorations for ${name}`);
					continue;
				}
				console.log(`Token type ${name} found ${ranges.length} times`);
				ed.setDecorations(decor, ranges);
			}
		}
	}));
	async function awaitClientNotStarting(): Promise<lsp.State> {
		while(true) {
			if (client.state !== lsp.State.Starting) return client.state;
			let sub: vsc.Disposable|undefined;
			await new Promise(r => sub = client.onDidChangeState(r, null));
			sub?.dispose();
		}
	}
	async function restartIfReady() {
		if (client.state == lsp.State.Running) await client.restart();
		else if (client.state == lsp.State.Stopped) await client.start();
	}
	async function restartWhenReady() {
		await awaitClientNotStarting();
		await restartIfReady();
	}
	function handleState() {
		if (vsc.workspace.getConfiguration().get("orchidls.output.preopen", false)) {

		}
		if (client.state == lsp.State.Stopped) {
			statusBarItem.text = "$(error) OrchidLS stopped"
		} else if (client.state == lsp.State.Starting) {
			statusBarItem.text = `$(loading~spin) OrchidLS`;
		} else {
			statusBarItem.text = "$(check) OrchidLS";
		}
	}
	function updateTrace() {
		context.globalState
		const tracestr = vsc.workspace.getConfiguration().get("orchidls.trace.server", "verbose");
		const trace = lsp.Trace.fromString(tracestr);
		console.log(`Orchid LS trace is ${trace}`);
		if (trace !== undefined) client.setTrace(trace).catch(console.error);
	}
	handleState();
	// In debug mode, watch the server executable.
	if (context.extensionMode !== vsc.ExtensionMode.Production) {
		console.log("Watching ls exe");
		const ls_watcher = fs.watch(LS_PATH);
		ls_watcher.on("change", async () => {
			console.log("Reloading ls exe")
			await restartWhenReady().catch(() => {});
			await restartWhenReady().catch(console.error);
		});
		const grammar_watcher = fs.watch(GRAMMAR_PATH);
		grammar_watcher.on("change", () => {
			vsc.commands.executeCommand("workbench.action.reloadWindow");
		});
		context.subscriptions.push({ dispose: () => {
			ls_watcher.close();
			grammar_watcher.close();
	 }});
	}
	updateTrace();
	context.subscriptions.push(vsc.workspace.onDidChangeConfiguration(updateTrace));
	// The command has been defined in the package.json file
	// Now provide the implementation of the command with registerCommand
	// The commandId parameter must match the command field in package.json
	context.subscriptions.push(vsc.commands.registerCommand('orchidls.restart-server', async () => {
		await restartWhenReady().catch(console.error);
		client.outputChannel.show();
		// channel.show();
	}));
}

// This method is called when your extension is deactivated
export function deactivate() {}
