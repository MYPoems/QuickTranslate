export type Language = "chinese" | "english";
export type OcrEngineKind = "windows" | "paddle" | "cloud";
export type OcrLanguage = "auto" | "chinese" | "english";
export interface SpeechPreferences {
  provider: "cloud" | "offline";
  cloudEndpoint: string; cloudModel: string;
  cloudChineseVoice: string; cloudEnglishVoice: string;
  threads: number;
  rate: number; chineseVoice: string; englishVoice: string; bilingual: boolean;
}
export interface SpeechVoice { id: string; name: string; language: string }
export interface SpeechAudio { audioDataUrl: string; voiceName: string }

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
  speech: SpeechPreferences;
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
  cloudSpeechApiKeyConfigured: boolean;
  paddleOcrInstalled: boolean;
  autoStartEnabled: boolean;
}

export interface UpdateSettings {
  speech: SpeechPreferences;
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
  cloudSpeechApiKey?: string;
  clearCloudSpeechApiKey: boolean;
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
  speech: SpeechPreferences;
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
export interface UpdateProgress {
  phase: "idle" | "checking" | "available" | "downloading" | "verifying" | "ready" | "installing" | "cancelled" | "error";
  version: string;
  downloaded: number;
  total?: number;
  message: string;
  releaseNotes: string;
}
export interface SpeechPluginStatus {
  installed: boolean;
  present: boolean;
  phase: "idle" | "downloading" | "verifying" | "ready" | "cancelled" | "error";
  downloaded: number;
  downloadBytes: number;
  version: string;
  message: string;
}
