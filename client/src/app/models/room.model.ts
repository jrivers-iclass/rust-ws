export interface Room {
  id: string;
  name: string;
  isSystem: boolean;
  isPrivate: boolean;
  hasPassword: boolean;
  messages: Message[];
  password?: string | null;
}

export interface Message {
  content: string;
  timestamp: Date;
  type: 'system' | 'user';
  sender?: string;
} 