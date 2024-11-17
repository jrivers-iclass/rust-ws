import { Injectable } from '@angular/core';
import { BehaviorSubject, Observable } from 'rxjs';
import { WebSocketService, WebSocketMessage } from './websocket.service';
import { Room, Message } from '../models/room.model';
import { map } from 'rxjs/operators';

@Injectable({
  providedIn: 'root'
})
export class ChatService {
  private currentRoom = new BehaviorSubject<Room | null>(null);
  private rooms = new BehaviorSubject<Room[]>([
    {
      id: 'general',
      name: 'General',
      isSystem: false,
      isPrivate: false,
      hasPassword: false,
      messages: [],
      password: null
    }
  ]);
  public rooms$ = this.rooms.asObservable();

  constructor(private wsService: WebSocketService) {
    // Subscribe to WebSocket messages
    this.wsService.messages$.subscribe((message: WebSocketMessage) => {
      console.log('ChatService received message:', message);
      // Temporarily handle both types until server is fixed
      if (message.action === 'publish' || message.action === 'message') {
        const normalizedMessage: WebSocketMessage = {
          ...message,
          action: 'publish'  // normalize the action type
        };
        this.handleIncomingMessage(normalizedMessage);
      }
    });
  }

  private handleIncomingMessage(message: WebSocketMessage) {
    console.log('Handling incoming message:', message);
    const currentRooms = this.rooms.getValue();
    const roomIndex = currentRooms.findIndex(r => r.id === message.topic);
    
    if (roomIndex !== -1) {
      // Create new message object
      const newMessage: Message = {
        content: message.message || '',
        timestamp: new Date(message.timestamp || Date.now()),
        type: 'user',
        sender: message.sender || 'Anonymous'
      };

      console.log('New message created:', newMessage);
      console.log('Current room state:', currentRooms[roomIndex]);

      // Create new room object with updated messages
      const updatedRoom = {
        ...currentRooms[roomIndex],
        messages: [...(currentRooms[roomIndex].messages || []), newMessage]
      };

      console.log('Updated room state:', updatedRoom);

      // Update rooms array
      const updatedRooms = [...currentRooms];
      updatedRooms[roomIndex] = updatedRoom;
      this.rooms.next(updatedRooms);

      // Update current room if needed
      const currentRoom = this.currentRoom.getValue();
      if (currentRoom && currentRoom.id === message.topic) {
        console.log('Updating current room with:', updatedRoom);
        this.currentRoom.next(updatedRoom);
      }
    } else {
      console.warn('Room not found for message:', message);
    }
  }

  getCurrentRoom(): Observable<Room | null> {
    return this.currentRoom.asObservable();
  }

  setCurrentRoom(room: Room) {
    console.log('Setting current room:', room);
    
    // Subscribe to the room's messages
    this.wsService.sendMessage({
      action: 'subscribe',
      topic: room.id
    });

    // Find the room in our rooms array
    const currentRooms = this.rooms.getValue();
    const existingRoom = currentRooms.find(r => r.id === room.id);
    
    console.log('Found existing room:', existingRoom);
    
    // Set the current room
    this.currentRoom.next(existingRoom || room);
  }

  publishMessage(roomId: string, content: string) {
    this.wsService.sendMessage({
      action: 'publish',
      topic: roomId,
      message: content
    });
  }

  getSystemMessages(): Observable<Message[]> {
    return this.rooms$.pipe(
      map(rooms => rooms.flatMap(room => room.messages || [])),
      map(messages => messages.filter((msg): msg is Message => 
        msg !== undefined && msg.type === 'system'
      ))
    );
  }

  createRoom(name: string, password: string | null) {
    const newRoom: Room = {
      id: crypto.randomUUID(),
      name,
      isSystem: false,
      isPrivate: !!password,
      hasPassword: !!password,
      messages: [],
      password
    };
    
    const currentRooms = this.rooms.getValue();
    this.rooms.next([...currentRooms, newRoom]);
  }
} 