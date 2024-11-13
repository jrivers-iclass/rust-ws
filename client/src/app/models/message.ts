export interface Message {
  sender: string;
  content: string;
  timestamp: Date;
  type?: 'system' | 'user';
} 