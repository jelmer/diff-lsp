import {
  ExtensionContext,
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  services,
  workspace
} from 'coc.nvim';

/**
 * Set up highlight links for semantic token types.
 *
 * coc.nvim creates highlight groups named CocSemType<tokenType> for each
 * semantic token type reported by the server. By default only standard LSP
 * types get linked, so we link the custom diff-lsp types to Vim groups.
 */
function setupSemanticHighlights(): void {
  const { nvim } = workspace;

  const links: Record<string, string> = {
    CocSemTypediffFileHeader: 'Statement',
    CocSemTypediffHunkHeader: 'Function',
    CocSemTypediffAddedLine: 'DiffAdd',
    CocSemTypediffDeletedLine: 'DiffDelete',
    CocSemTypediffContextLine: 'Comment',
  };

  for (const [group, target] of Object.entries(links)) {
    nvim.command(`hi default link ${group} ${target}`, true);
  }
}

export async function activate(context: ExtensionContext): Promise<void> {
  const config = workspace.getConfiguration('diff');
  const isEnable = config.get<boolean>('enable', true);

  if (!isEnable) {
    return;
  }

  setupSemanticHighlights();

  const serverPath = config.get<string>('serverPath', 'diff-lsp');

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: []
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'diff' },
      { scheme: 'file', pattern: '**/*.patch' },
      { scheme: 'file', pattern: '**/*.diff' },
    ],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher('**/*.{patch,diff}')
    }
  };

  const client = new LanguageClient(
    'diff',
    'Diff Language Server',
    serverOptions,
    clientOptions
  );

  context.subscriptions.push(services.registLanguageClient(client));
}
