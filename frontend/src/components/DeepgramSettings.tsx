'use client';

import { invoke } from '@tauri-apps/api/core';
import { useState, useEffect, useCallback } from 'react';
import { Key, CheckCircle2, AlertCircle, Loader2, ExternalLink, Eye, EyeOff } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';

interface DeepgramSettingsProps {
  className?: string;
  onConfigurationChange?: (configured: boolean) => void;
}

// Available Deepgram models
const DEEPGRAM_MODELS = [
  { value: 'nova-2', label: 'Nova 2', description: 'Latest, most accurate' },
  { value: 'nova-2-general', label: 'Nova 2 General', description: 'General purpose' },
  { value: 'nova-2-meeting', label: 'Nova 2 Meeting', description: 'Optimized for meetings' },
  { value: 'nova-2-phonecall', label: 'Nova 2 Phone Call', description: 'Phone conversations' },
  { value: 'whisper-medium', label: 'Whisper Medium', description: 'OpenAI Whisper via Deepgram' },
  { value: 'whisper-large', label: 'Whisper Large', description: 'Highest accuracy Whisper' },
];

// Supported languages
const LANGUAGES = [
  { value: 'en', label: 'English' },
  { value: 'es', label: 'Spanish' },
  { value: 'fr', label: 'French' },
  { value: 'de', label: 'German' },
  { value: 'it', label: 'Italian' },
  { value: 'pt', label: 'Portuguese' },
  { value: 'nl', label: 'Dutch' },
  { value: 'ja', label: 'Japanese' },
  { value: 'ko', label: 'Korean' },
  { value: 'zh', label: 'Chinese' },
];

export default function DeepgramSettings({ className = '', onConfigurationChange }: DeepgramSettingsProps) {
  const [apiKey, setApiKey] = useState('');
  const [showApiKey, setShowApiKey] = useState(false);
  const [isConfigured, setIsConfigured] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [saveStatus, setSaveStatus] = useState<'idle' | 'success' | 'error'>('idle');
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // Advanced options
  const [model, setModel] = useState('nova-2');
  const [language, setLanguage] = useState('en');
  const [punctuate, setPunctuate] = useState(true);
  const [diarize, setDiarize] = useState(false);
  const [smartFormat, setSmartFormat] = useState(true);

  // Check initial configuration
  useEffect(() => {
    const checkConfiguration = async () => {
      try {
        await invoke('init_deepgram_provider');
        const configured = await invoke<boolean>('is_deepgram_configured');
        setIsConfigured(configured);
        if (onConfigurationChange) {
          onConfigurationChange(configured);
        }
      } catch (err) {
        console.error('Failed to check Deepgram configuration:', err);
      }
    };
    checkConfiguration();
  }, [onConfigurationChange]);

  // Save API key
  const handleSaveApiKey = useCallback(async () => {
    if (!apiKey.trim()) {
      setErrorMessage('Please enter an API key');
      setSaveStatus('error');
      return;
    }

    setIsSaving(true);
    setSaveStatus('idle');
    setErrorMessage(null);

    try {
      await invoke('set_deepgram_api_key', { apiKey: apiKey.trim() });
      setIsConfigured(true);
      setSaveStatus('success');
      setApiKey(''); // Clear input after saving

      if (onConfigurationChange) {
        onConfigurationChange(true);
      }

      // Reset status after 3 seconds
      setTimeout(() => setSaveStatus('idle'), 3000);
    } catch (err) {
      console.error('Failed to save Deepgram API key:', err);
      setErrorMessage(`Failed to save: ${err}`);
      setSaveStatus('error');
    } finally {
      setIsSaving(false);
    }
  }, [apiKey, onConfigurationChange]);

  // Save options
  const handleSaveOptions = useCallback(async () => {
    try {
      await invoke('set_deepgram_options', {
        model,
        language,
        diarize,
        punctuate,
        smartFormat,
      });
      setSaveStatus('success');
      setTimeout(() => setSaveStatus('idle'), 2000);
    } catch (err) {
      console.error('Failed to save Deepgram options:', err);
      setErrorMessage(`Failed to save options: ${err}`);
      setSaveStatus('error');
    }
  }, [model, language, diarize, punctuate, smartFormat]);

  return (
    <div className={`flex flex-col gap-4 p-4 rounded-lg border bg-card ${className}`}>
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2 text-lg font-semibold">
          <Key className="h-5 w-5" />
          <span>Deepgram API Settings</span>
        </div>
        {isConfigured && (
          <div className="flex items-center gap-1 text-sm text-green-600">
            <CheckCircle2 className="h-4 w-4" />
            <span>Configured</span>
          </div>
        )}
      </div>

      <p className="text-sm text-muted-foreground">
        Deepgram provides fast, accurate cloud-based transcription.
        <a
          href="https://deepgram.com"
          target="_blank"
          rel="noopener noreferrer"
          className="ml-1 text-primary hover:underline inline-flex items-center gap-1"
        >
          Get your API key
          <ExternalLink className="h-3 w-3" />
        </a>
      </p>

      {/* API Key Input */}
      <div className="flex flex-col gap-2">
        <Label htmlFor="deepgram-api-key">API Key</Label>
        <div className="flex gap-2">
          <div className="relative flex-1">
            <Input
              id="deepgram-api-key"
              type={showApiKey ? 'text' : 'password'}
              placeholder={isConfigured ? '••••••••••••••••' : 'Enter your Deepgram API key'}
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              disabled={isSaving}
              className="pr-10"
            />
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="absolute right-0 top-0 h-full px-3"
              onClick={() => setShowApiKey(!showApiKey)}
            >
              {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
            </Button>
          </div>
          <Button
            onClick={handleSaveApiKey}
            disabled={isSaving || !apiKey.trim()}
          >
            {isSaving ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              'Save'
            )}
          </Button>
        </div>
        {saveStatus === 'success' && (
          <p className="text-sm text-green-600 flex items-center gap-1">
            <CheckCircle2 className="h-4 w-4" />
            API key saved successfully
          </p>
        )}
        {saveStatus === 'error' && errorMessage && (
          <p className="text-sm text-destructive flex items-center gap-1">
            <AlertCircle className="h-4 w-4" />
            {errorMessage}
          </p>
        )}
      </div>

      {/* Advanced Options - Only show if configured */}
      {isConfigured && (
        <>
          <div className="border-t pt-4 mt-2">
            <h4 className="font-medium mb-3">Transcription Options</h4>

            {/* Model Selection */}
            <div className="flex flex-col gap-2 mb-4">
              <Label>Model</Label>
              <Select value={model} onValueChange={setModel}>
                <SelectTrigger>
                  <SelectValue placeholder="Select model" />
                </SelectTrigger>
                <SelectContent>
                  {DEEPGRAM_MODELS.map((m) => (
                    <SelectItem key={m.value} value={m.value}>
                      <div className="flex flex-col">
                        <span>{m.label}</span>
                        <span className="text-xs text-muted-foreground">{m.description}</span>
                      </div>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            {/* Language Selection */}
            <div className="flex flex-col gap-2 mb-4">
              <Label>Language</Label>
              <Select value={language} onValueChange={setLanguage}>
                <SelectTrigger>
                  <SelectValue placeholder="Select language" />
                </SelectTrigger>
                <SelectContent>
                  {LANGUAGES.map((lang) => (
                    <SelectItem key={lang.value} value={lang.value}>
                      {lang.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            {/* Toggle Options */}
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <div>
                  <Label>Punctuation</Label>
                  <p className="text-xs text-muted-foreground">Add punctuation to transcript</p>
                </div>
                <Switch checked={punctuate} onCheckedChange={setPunctuate} />
              </div>

              <div className="flex items-center justify-between">
                <div>
                  <Label>Speaker Diarization</Label>
                  <p className="text-xs text-muted-foreground">Identify different speakers</p>
                </div>
                <Switch checked={diarize} onCheckedChange={setDiarize} />
              </div>

              <div className="flex items-center justify-between">
                <div>
                  <Label>Smart Formatting</Label>
                  <p className="text-xs text-muted-foreground">Format numbers, dates, etc.</p>
                </div>
                <Switch checked={smartFormat} onCheckedChange={setSmartFormat} />
              </div>
            </div>

            <Button
              onClick={handleSaveOptions}
              variant="outline"
              className="mt-4 w-full"
            >
              Save Options
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
