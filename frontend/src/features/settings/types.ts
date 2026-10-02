import type { BrowserModel, ModelDownloads } from '../models/types.ts';

export type SettingsView = 'settings' | 'hardware' | 'shortcuts' | null;
export type InterfaceDensity = 'compact' | 'comfortable' | 'large';
export interface SetupInstallation { id?: string; name?: string; path: string; startable?: boolean }
export interface SetupFile { name: string; label?: string; folder?: string; exists: boolean; bytes?: number; expected_bytes?: number }
export interface SetupJob { status: 'running' | 'complete' | 'error' | string; action?: string; message?: string; error?: string; phase?: string; progress?: number | null; downloaded_bytes?: number; total_bytes?: number }
export interface SetupState {
  installation?: SetupInstallation | null;
  installations?: SetupInstallation[];
  install_directory?: string;
  managed_directory?: string;
  configured_ai_directory?: string;
  model_directory?: string;
  setup_mode?: string;
  models?: SetupFile[];
  job?: SetupJob | null;
  service?: { running?: boolean; ready?: boolean; starting?: boolean; busy?: boolean; can_start?: boolean; can_eject?: boolean; reason?: string; port?: number; device?: string; qwen_ready?: boolean; flux_ready?: boolean };
  portable?: { available?: boolean; download_bytes?: number; minimum_free_bytes?: number };
  storage?: { portable_folder?: { free_bytes?: number; error?: string }; model_folder?: { free_bytes?: number; error?: string } };
  model_folder_connection?: { status?: string; message?: string };
}
export interface RemovalSettings { model?: string; models: Array<{ id: string; label?: string; available?: boolean; [key: string]: unknown }> }
export interface BackendStatus { ready?: boolean; retouch_ready?: boolean; device?: string; [key: string]: unknown }
export interface HardwareGuide { devices?: Array<{ name: string; vram_gb?: number }>; system_ram_gb?: number; note?: string; profiles?: Array<{ label: string; vram: string; detail?: string; basis?: string; source_url?: string }> }
export interface HardwarePreference { dont_show_again: boolean | null }
export interface AcceptedConfiguration { settings?: RemovalSettings; status?: BackendStatus; qwen?: Record<string, unknown> | null; setup?: SetupState }
export interface SettingsCapabilities { ready: boolean; setup: boolean }
export interface SettingsPreferences { askBeforeOverwrite: boolean; density: InterfaceDensity }
export type SetupAction = 'chooseRuntime' | 'chooseInstallDirectory' | 'installRuntime' | 'chooseModelDirectory' | 'downloadRemovalModels' | 'downloadModel' | 'startBackend' | 'ejectModels' | 'useInstallation' | 'configureConnection';
export interface SettingsBridge {
  capabilities(): SettingsCapabilities;
  editorBusy(): boolean;
  preferences(): SettingsPreferences;
  acceptConfiguration(value: AcceptedConfiguration): void;
  setOperationPending(value: boolean): void;
  setOverwritePreference(value: boolean): void;
  setDensity(value: InterfaceDensity): void;
  focusCanvas(): void;
  chooseRuntime(): Promise<unknown | null>;
  chooseInstallDirectory(): Promise<unknown | null>;
  installRuntime(): Promise<unknown | null>;
  chooseModelDirectory(): Promise<unknown | null>;
  downloadRemovalModels(): Promise<unknown | null>;
  downloadModel(model: string, variant: string): Promise<unknown | null>;
  startBackend(): Promise<unknown | null>;
  ejectModels(): Promise<unknown | null>;
  useInstallation(id: string): Promise<unknown | null>;
  configureConnection(): Promise<unknown | null>;
}
export interface SettingsSnapshot {
  view: SettingsView;
  requestedSection: 'general' | 'ai';
  loading: boolean;
  pendingAction: SetupAction | null;
  setup: SetupState | null;
  models: BrowserModel[];
  modelCatalogAvailable: boolean;
  modelDownloads: ModelDownloads | null;
  selectedModelId: string;
  selectedVariant: string;
  hardware: HardwareGuide | null;
  hideHardwareGuide: boolean;
  savingHardwarePreference: boolean;
  capabilities: SettingsCapabilities;
  preferences: SettingsPreferences;
  error: string;
  message: string;
  showInstallations: boolean;
  selectedInstallation: string;
}
declare global { interface Window { LocalImageSettingsBridge?: SettingsBridge } }
