import { describe, expect, it } from 'vitest';
import { recipesForDiff } from './recipescope';
import type { Recipe } from './types';

const r = (id: string, kind: 'llm' | 'tool' = 'llm'): Recipe => ({ id, title: id, source: 'builtin', prompt: '', kind });
const CODE = ['bugs', 'errors', 'security', 'performance', 'tests', 'consistency'];
const TF = ['tf-state', 'tf-replace', 'tf-repro', 'tf-security', 'tf-refs'];
const DOC = ['doc-claims', 'doc-reality', 'doc-steps'];
const all = [...CODE, ...TF, ...DOC].map((id) => r(id)).concat(r('lint', 'tool'), r('mine'));
const ids = (files: string[]): string[] => recipesForDiff(all, files.map((path) => ({ path }))).map((x) => x.id);

describe('recipesForDiff', () => {
  it('コードだけの差分では、Terraform を出さず、ほかは全部出す', () => {
    expect(ids(['src/a.ts', 'README.md'])).toEqual([...CODE, ...DOC, 'lint', 'mine']);
  });

  it('Terraform だけの差分では、コードの観点を隠し、Terraform の 5 つとリンターだけ出す', () => {
    expect(ids(['infra/main.tf', 'infra/prod.tfvars', '.terraform.lock.hcl'])).toEqual([...TF, 'lint', 'mine']);
  });

  it('文書・設定が混ざっても、数えるファイルが Terraform だけなら、Terraform だけにする', () => {
    expect(ids(['infra/main.tf', 'README.md', '.github/workflows/plan.yml', 'Makefile'])).toEqual([...TF, ...DOC, 'lint', 'mine']);
  });

  it('文書・設定だけの差分は、コードの扱い（Terraform を出さない）に、文書の観点を足す', () => {
    expect(ids(['README.md', 'config.yml'])).toEqual([...CODE, ...DOC, 'lint', 'mine']);
  });

  it('コードと Terraform が混ざる差分では、全部出す', () => {
    expect(ids(['src/a.ts', 'infra/main.tf', 'docs/a.md'])).toHaveLength(all.length);
  });

  it('差分が空のときは、何に向くか判断できないので、全部の観点を出す', () => {
    expect(ids([])).toEqual(all.map((x) => x.id));
    for (const t of [...TF, ...DOC]) expect(ids([])).toContain(t);
  });

  it('拡張子は大文字小文字を区別せず、ディレクトリ名の . は拡張子にしない', () => {
    expect(ids(['MAIN.TF'])).toContain('tf-state');
    expect(ids(['modules.tf/readme'])).not.toContain('tf-state');
  });

  it('自分で足した観点とリンターは、いつも出す', () => {
    expect(ids(['a.tf'])).toEqual(expect.arrayContaining(['lint', 'mine']));
  });

  it('文書の観点は、文書（md・mdx・txt・rst）が差分にあるときだけ出し、コードの観点は隠さない', () => {
    for (const f of ['README.md', 'a/b.MDX', 'notes.txt', 'doc/index.rst']) {
      expect(ids([f])).toEqual(expect.arrayContaining(DOC));
      expect(ids([f])).toEqual(expect.arrayContaining(CODE));
    }
    for (const d of DOC) expect(ids(['src/a.ts'])).not.toContain(d);
    for (const d of DOC) expect(ids(['config.yml', 'package.json'])).not.toContain(d);
  });
});
