/** Mirror of `lazybones_store::Asset` — content-addressed file metadata. The
 *  bytes live behind the blob store; this is metadata only. Identical bytes
 *  dedup to one asset (reusable images for free). */
export interface Asset {
  id: string;
  project?: string | null;
  filename: string;
  content_type: string;
  size: number;
  sha256: string;
  created_at: string;
}
