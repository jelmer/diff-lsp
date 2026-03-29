import { workspace } from 'coc.nvim';

describe('coc-diff', () => {
  it('should have correct configuration properties', () => {
    const packageJson = require('../package.json');
    const config = packageJson.contributes.configuration.properties;

    expect(config['diff.enable']).toBeDefined();
    expect(config['diff.enable'].type).toBe('boolean');
    expect(config['diff.enable'].default).toBe(true);

    expect(config['diff.serverPath']).toBeDefined();
    expect(config['diff.serverPath'].type).toBe('string');
    expect(config['diff.serverPath'].default).toBe('diff-lsp');
  });

  it('should have correct semantic token types', () => {
    const packageJson = require('../package.json');
    const tokenTypes = packageJson.contributes.semanticTokenTypes;

    const ids = tokenTypes.map((t: any) => t.id);
    expect(ids).toContain('diffFileHeader');
    expect(ids).toContain('diffHunkHeader');
    expect(ids).toContain('diffAddedLine');
    expect(ids).toContain('diffDeletedLine');
    expect(ids).toContain('diffContextLine');
  });

  it('should activate on diff language', () => {
    const packageJson = require('../package.json');
    expect(packageJson.activationEvents).toContain('onLanguage:diff');
  });
});
