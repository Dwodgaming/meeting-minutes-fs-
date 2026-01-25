'use client';

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';
import { useState, useEffect, useCallback } from 'react';
import { FileAudio, Upload, Loader2, CheckCircle2, AlertCircle, Copy, Download, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import { ScrollArea } from '@/components/ui/scroll-area';

// Types matching Rust backend
interface AudioFileInfo {
  duration_seconds: number;
  sample_rate: number;
  channels: number;
  format: string;
  file_size_bytes: number;
}

interface FileTranscriptionResult {
  text: string;
  duration_seconds: number;
  provider: string;
  model: string | null;
  confidence: number | null;
  file_info: AudioFileInfo;
}

interface TranscriptionProgress {
  stage: string;
  progress: number;
  message: string;
}

type TranscriptionProvider = 'whisper' | 'parakeet' | 'deepgram';

interface FileTranscriptionProps {
  onTranscriptionComplete?: (result: FileTranscriptionResult) => void;
  className?: string;
}

export default function FileTranscription({ onTranscriptionComplete, className = '' }: FileTranscriptionProps) {
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [fileInfo, setFileInfo] = useState<AudioFileInfo | null>(null);
  const [provider, setProvider] = useState<TranscriptionProvider>('whisper');
  const [isTranscribing, setIsTranscribing] = useState(false);
  const [progress, setProgress] = useState<TranscriptionProgress | null>(null);
  const [result, setResult] = useState<FileTranscriptionResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [deepgramConfigured, setDeepgramConfigured] = useState(false);

  // Check if Deepgram is configured
  useEffect(() => {
    const checkDeepgram = async () => {
      try {
        await invoke('init_deepgram_provider');
        const configured = await invoke<boolean>('is_deepgram_configured');
        setDeepgramConfigured(configured);
      } catch (err) {
        console.error('Failed to check Deepgram configuration:', err);
      }
    };
    checkDeepgram();
  }, []);

  // Listen for transcription progress events
  useEffect(() => {
    const unlisten = listen<TranscriptionProgress>('file-transcription-progress', (event) => {
      setProgress(event.payload);
    });

    return () => {
      unlisten.then(fn => fn());
    };
  }, []);

  // Format file size for display
  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  // Format duration for display
  const formatDuration = (seconds: number): string => {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    if (mins === 0) return `${secs}s`;
    return `${mins}m ${secs}s`;
  };

  // Select file handler
  const handleSelectFile = useCallback(async () => {
    try {
      setError(null);
      setResult(null);
      setFileInfo(null);

      const selected = await open({
        multiple: false,
        filters: [{
          name: 'Audio/Video',
          extensions: ['mp3', 'wav', 'm4a', 'mp4', 'webm', 'ogg', 'flac', 'aac', 'opus']
        }]
      });

      if (selected && typeof selected === 'string') {
        setSelectedFile(selected);

        // Validate file and get info
        try {
          const info = await invoke<AudioFileInfo>('validate_file_for_transcription', {
            filePath: selected
          });
          setFileInfo(info);
        } catch (validationError) {
          setError(`Invalid file: ${validationError}`);
          setSelectedFile(null);
        }
      }
    } catch (err) {
      console.error('File selection error:', err);
      setError(`Failed to select file: ${err}`);
    }
  }, []);

  // Transcribe file handler
  const handleTranscribe = useCallback(async () => {
    if (!selectedFile) return;

    setIsTranscribing(true);
    setError(null);
    setResult(null);
    setProgress({ stage: 'starting', progress: 0, message: 'Starting transcription...' });

    try {
      const transcriptionResult = await invoke<FileTranscriptionResult>('transcribe_file', {
        filePath: selectedFile,
        provider: provider,
        language: null // Use auto-detect
      });

      setResult(transcriptionResult);
      setProgress(null);

      if (onTranscriptionComplete) {
        onTranscriptionComplete(transcriptionResult);
      }
    } catch (err) {
      console.error('Transcription error:', err);
      setError(`Transcription failed: ${err}`);
      setProgress(null);
    } finally {
      setIsTranscribing(false);
    }
  }, [selectedFile, provider, onTranscriptionComplete]);

  // Copy transcript to clipboard
  const handleCopyTranscript = useCallback(() => {
    if (result?.text) {
      navigator.clipboard.writeText(result.text);
    }
  }, [result]);

  // Clear selection
  const handleClear = useCallback(() => {
    setSelectedFile(null);
    setFileInfo(null);
    setResult(null);
    setError(null);
    setProgress(null);
  }, []);

  // Get filename from path
  const getFileName = (path: string): string => {
    const parts = path.split(/[/\\]/);
    return parts[parts.length - 1] || path;
  };

  return (
    <div className={`flex flex-col gap-4 p-4 rounded-lg border bg-card ${className}`}>
      <div className="flex items-center gap-2 text-lg font-semibold">
        <FileAudio className="h-5 w-5" />
        <span>Transcribe Audio/Video File</span>
      </div>

      {/* File Selection */}
      <div className="flex flex-col gap-2">
        <Label>Audio/Video File</Label>
        <div className="flex gap-2">
          <Button
            variant="outline"
            className="flex-1 justify-start"
            onClick={handleSelectFile}
            disabled={isTranscribing}
          >
            <Upload className="h-4 w-4 mr-2" />
            {selectedFile ? getFileName(selectedFile) : 'Select file...'}
          </Button>
          {selectedFile && (
            <Button
              variant="ghost"
              size="icon"
              onClick={handleClear}
              disabled={isTranscribing}
            >
              <X className="h-4 w-4" />
            </Button>
          )}
        </div>

        {/* File Info */}
        {fileInfo && (
          <div className="text-sm text-muted-foreground flex gap-4">
            <span>{formatDuration(fileInfo.duration_seconds)}</span>
            <span>{formatFileSize(fileInfo.file_size_bytes)}</span>
            <span>{fileInfo.format.toUpperCase()}</span>
            <span>{fileInfo.sample_rate} Hz</span>
          </div>
        )}
      </div>

      {/* Provider Selection */}
      <div className="flex flex-col gap-2">
        <Label>Transcription Provider</Label>
        <Select
          value={provider}
          onValueChange={(value) => setProvider(value as TranscriptionProvider)}
          disabled={isTranscribing}
        >
          <SelectTrigger>
            <SelectValue placeholder="Select provider" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="whisper">
              <div className="flex items-center gap-2">
                <span>Whisper</span>
                <span className="text-xs text-muted-foreground">(Local)</span>
              </div>
            </SelectItem>
            <SelectItem value="parakeet">
              <div className="flex items-center gap-2">
                <span>Parakeet</span>
                <span className="text-xs text-muted-foreground">(Local, Fast)</span>
              </div>
            </SelectItem>
            <SelectItem value="deepgram" disabled={!deepgramConfigured}>
              <div className="flex items-center gap-2">
                <span>Deepgram</span>
                <span className="text-xs text-muted-foreground">
                  {deepgramConfigured ? '(Cloud API)' : '(Not configured)'}
                </span>
              </div>
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      {/* Transcribe Button */}
      <Button
        onClick={handleTranscribe}
        disabled={!selectedFile || isTranscribing}
        className="w-full"
        variant={isTranscribing ? 'secondary' : 'default'}
      >
        {isTranscribing ? (
          <>
            <Loader2 className="h-4 w-4 mr-2 animate-spin" />
            Transcribing...
          </>
        ) : (
          <>
            <FileAudio className="h-4 w-4 mr-2" />
            Transcribe File
          </>
        )}
      </Button>

      {/* Progress */}
      {progress && (
        <div className="flex flex-col gap-2">
          <Progress value={progress.progress} className="h-2" />
          <div className="text-sm text-muted-foreground flex justify-between">
            <span>{progress.message}</span>
            <span>{progress.progress}%</span>
          </div>
        </div>
      )}

      {/* Error */}
      {error && (
        <div className="flex items-start gap-2 p-3 rounded-md bg-destructive/10 text-destructive">
          <AlertCircle className="h-5 w-5 shrink-0 mt-0.5" />
          <span className="text-sm">{error}</span>
        </div>
      )}

      {/* Result */}
      {result && (
        <div className="flex flex-col gap-3">
          <div className="flex items-center gap-2 text-green-600">
            <CheckCircle2 className="h-5 w-5" />
            <span className="font-medium">Transcription Complete</span>
          </div>

          <div className="text-sm text-muted-foreground flex gap-4">
            <span>Duration: {formatDuration(result.duration_seconds)}</span>
            <span>Provider: {result.provider}</span>
            {result.model && <span>Model: {result.model}</span>}
            {result.confidence !== null && (
              <span>Confidence: {(result.confidence * 100).toFixed(1)}%</span>
            )}
          </div>

          <div className="flex gap-2">
            <Button variant="outline" size="sm" onClick={handleCopyTranscript}>
              <Copy className="h-4 w-4 mr-2" />
              Copy Transcript
            </Button>
          </div>

          <ScrollArea className="h-48 rounded-md border p-3 bg-muted/50">
            <p className="text-sm whitespace-pre-wrap">{result.text}</p>
          </ScrollArea>
        </div>
      )}
    </div>
  );
}
