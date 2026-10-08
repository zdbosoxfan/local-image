import type { ModelFile } from './types.ts';
import './modelFiles.css';

const roles: Record<string, string> = {
  diffusion_models: 'Image model',
  text_encoders: 'Text encoder',
  vae: 'VAE',
  upscale_models: 'Upscaler',
  loras: 'Adapter',
};
const bytes = (value: number) =>
  value >= 1024 ** 3 ? `${(value / 1024 ** 3).toFixed(2)} GB` : `${(value / 1024 ** 2).toFixed(1)} MB`;

export function ModelFiles({ files }: { files: readonly ModelFile[] }) {
  if (!files.length) return null;
  return (
    <section className="li-model-files" aria-label="Included model download files">
      <strong>Download includes</strong>
      <p>
        The selected model and all required companion files are downloaded together, including its VAE and text encoder.
        Existing files are checked and reused.
      </p>
      <ul>
        {files.map(file => (
          <li key={`${file.folder}/${file.name}`}>
            <div>
              <strong>{roles[file.folder] || 'Companion model'}</strong>
              <span>
                {file.folder}/{file.name}
              </span>
            </div>
            <span>
              {bytes(file.bytes)} · {file.exists ? 'Already present' : 'To download'}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}
