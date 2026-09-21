export type Language = "chinese" | "english";
export type OcrEngineKind = "windows" | "paddle" | "cloud";
export type OcrLanguage = "auto" | "chinese" | "english";

export interface TranslationResult {
  sourceText: string;
  translation: string;
  detectedLanguage: Language;
  targetLanguage: Language;
  provider: string;
  model: string;
  cached: boolean;
  phonetic?: string;
  partOfSpeech?: string;
  definitions?: string[];
  example?: string;
}

export interface AppError {
  code: string;
  message: string;
}

export interface TranslationEvent {
  requestId: number;
  status: "loading" | "success" | "error";
  sourceText?: string;
  sourceKind?: "selection" | "ocr";
  result?: TranslationResult;
  error?: AppError;
}

export interface SettingsView {
  provider: string;
  baseUrl: string;
  model: string;
  globalShortcut: string;
  ocrShortcut: string;
  ocrEngine: OcrEngineKind;
  ocrLanguage: OcrLanguage;
  cloudOcrBaseUrl: string;
  cloudOcrModel: string;
  apiKeyConfigured: boolean;
  cloudOcrApiKeyConfigured: boolean;
  paddleOcrInstalled: boolean;
  autoStartEnabled: boolean;
}

export interface UpdateSettings {
  provider: string;
  baseUrl: string;
  model: string;
  globalShortcut: string;
  ocrShortcut: string;
  ocrEngine: OcrEngineKind;
  ocrLanguage: OcrLanguage;
  cloudOcrBaseUrl: string;
  cloudOcrModel: string;
  apiKey?: string;
  clearApiKey: boolean;
  cloudOcrApiKey?: string;
  clearCloudOcrApiKey: boolean;
  autoStartEnabled: boolean;
}

export interface DiagnosticsView {
  appVersion: string;
  provider: string;
  baseUrl: string;
  model: string;
  ocrEngine: OcrEngineKind;
  ocrLanguage: OcrLanguage;
  apiKeyConfigured: boolean;
  cloudOcrModel: string;
  cloudOcrApiKeyConfigured: boolean;
  paddleOcrInstalled: boolean;
  cacheEntries: number;
  settingsPath: string;
  cachePath: string;
  lastError?: AppError;
}

export interface HistoryEntry {
  id: number;
  sourceText: string;
  translation: string;
  sourceLanguage: string;
  targetLanguage: string;
  provider: string;
  model: string;
  createdAt: number;
  favorite: boolean;
}

export interface SettingsBackup {
  schemaVersion: number;
  provider: string;
  baseUrl: string;
  model: string;
  globalShortcut: string;
  ocrShortcut: string;
  ocrEngine: OcrEngineKind;
  ocrLanguage: OcrLanguage;
  cloudOcrBaseUrl: string;
  cloudOcrModel: string;
  autoStartEnabled: boolean;
}

export interface PaddleOcrPluginStatus {
  installed: boolean;
  version: string;
  installedBytes: number;
  downloadBytes: number;
}

export type OcrRegionResult =
  | { kind: "completed"; requestId: number }
  | {
      kind: "paddle";
      requestId: number;
      imageDataUrl: string;
      detectionModelPath: string;
      recognitionModelPath: string;
    };

export interface UpdateInfo {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  releaseUrl: string;
}
