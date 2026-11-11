import type { Recipe } from './types';

/** Terraform の差分とみなす拡張子（`.terraform.lock.hcl` も `hcl`）。 */
export const TERRAFORM_EXTS: readonly string[] = ['tf', 'tfvars', 'hcl'];

/** 観点の塊の見出し。日英とも同じ（製品名）。 */
export const TERRAFORM_LABEL = 'Terraform';

/** Terraform の観点（組み込み）。差分に Terraform のファイルがあるときだけ出す。 */
export const TERRAFORM_IDS: readonly string[] = ['tf-state', 'tf-replace', 'tf-repro', 'tf-security', 'tf-refs'];

/** 文書の差分とみなす拡張子。 */
export const DOC_EXTS: readonly string[] = ['md', 'mdx', 'txt', 'rst'];

/** 文書の観点（組み込み）。差分に文書のファイルがあるときだけ、追加で出す（コードの観点は、これまでどおり隠さない）。 */
export const DOC_IDS: readonly string[] = ['doc-claims', 'doc-reality', 'doc-steps'];

/** コードの観点（組み込み）。Terraform だけの差分では出さない（インフラの担当に、コードの観点は要らない。見たいときは「すべての観点」）。 */
const CODE_IDS: readonly string[] = ['bugs', 'errors', 'security', 'performance', 'tests', 'consistency'];

const extOf = (path: string): string => {
  const base = path.split('/').pop() ?? path;
  const i = base.lastIndexOf('.');
  return i < 0 ? '' : base.slice(i + 1).toLowerCase();
};

/**
 * コードか Terraform かの判定に、数えないファイル（文書・設定・拡張子なし）。これが混ざるだけで「混在」にしない。
 * 向く・向かないの見立てで、未検証。`.yml` / `.json` のうち、別の領域のもの（Kubernetes など）は、まだ区別しない。
 */
const NEUTRAL_EXTS: readonly string[] = ['md', 'mdx', 'txt', 'rst', 'yml', 'yaml', 'json', 'toml', 'lock', 'gitignore', ''];

const isTerraformFile = (path: string): boolean => TERRAFORM_EXTS.includes(extOf(path));

const isDocFile = (path: string): boolean => DOC_EXTS.includes(extOf(path));

/**
 * この差分に向く観点だけに絞る。組み込みの id だけで決め、それ以外（自分で足した観点・リンター）は、いつも出す。
 * 文書・設定（`.md`・`.yml` など）は数えない。数えるファイルが Terraform だけの差分 → Terraform の観点（とリンター）だけ。コードか混在 → コードの観点（混在なら Terraform も）。
 * 差分に文書（`.md`・`.mdx`・`.txt`・`.rst`）があれば、文書の観点を追加で出す（コードの観点は、隠さない）。
 * 差分が空のときは、何に向くか判断できないので、隠さず、全部の観点を出す（拡張子などからの見立ては、差分がある場合だけ使う。見立てを誤っても、「すべての観点」で全部出せる）。
 */
export function recipesForDiff(recipes: Recipe[], files: { path: string }[]): Recipe[] {
  if (files.length === 0) return recipes;
  const hasTf = files.some((f) => isTerraformFile(f.path));
  const hasDoc = files.some((f) => isDocFile(f.path));
  const counted = files.filter((f) => !NEUTRAL_EXTS.includes(extOf(f.path)) || isTerraformFile(f.path));
  const tfOnly = counted.length > 0 && counted.every((f) => isTerraformFile(f.path));
  return recipes.filter((r) => {
    if (TERRAFORM_IDS.includes(r.id)) return hasTf;
    if (DOC_IDS.includes(r.id)) return hasDoc;
    if (CODE_IDS.includes(r.id)) return !tfOnly;
    return true;
  });
}
