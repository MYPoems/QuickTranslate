export type Language = "chinese" | "english";

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
  result?: TranslationResult;
  error?: AppError;
}

export interface SettingsView {
  provider: string;
  baseUrl: string;
  model: string;
  globalShortcut: string;
  apiKeyConfigured: boolean;
  autoStartEnabled: boolean;
}

export interface UpdateSettings {
  provider: string;
  baseUrl: string;
  model: string;
  globalShortcut: string;
  apiKey?: string;
  clearApiKey: boolean;
  autoStartEnabled: boolean;
}
