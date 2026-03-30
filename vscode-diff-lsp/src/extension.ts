import { workspace, ExtensionContext } from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
  const config = workspace.getConfiguration('diff');
  const isEnable = config.get<boolean>('enable', true);

  if (!isEnable) {
    return;
  }

  const serverPath = config.get<string>('serverPath', 'diff-lsp');

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: [],
    transport: TransportKind.stdio
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'diff' },
      { scheme: 'file', language: 'quiltseries' },
      { scheme: 'file', pattern: '**/*.patch' },
      { scheme: 'file', pattern: '**/*.diff' },
      { scheme: 'file', pattern: '**/series' },
      { scheme: 'file', pattern: '**/series.conf' },
      { scheme: 'file', pattern: '**/patches/**' },
    ],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher(
        '**/{*.patch,*.diff,series,series.conf,patches/**}'
      )
    }
  };

  client = new LanguageClient(
    'diff',
    'Diff Language Server',
    serverOptions,
    clientOptions
  );

  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
