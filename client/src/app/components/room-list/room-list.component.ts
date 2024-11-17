import { Component, OnInit } from '@angular/core';
import { ChatService } from '../../services/chat.service';
import { Observable, map } from 'rxjs';
import { Room } from '../../models/room.model';

@Component({
  selector: 'app-room-list',
  templateUrl: './room-list.component.html',
  styleUrls: ['./room-list.component.scss']
})
export class RoomListComponent implements OnInit {
  rooms$: Observable<Room[]>;
  currentRoom$: Observable<Room | null>;

  constructor(private chatService: ChatService) {
    this.rooms$ = this.chatService.rooms$.pipe(
      map((rooms: Room[]) => {
        const existingRooms = rooms.filter((r: Room) => r.id !== 'general');
        return [
          {
            id: 'general',
            name: 'General',
            isSystem: true,
            isPrivate: false,
            hasPassword: false,
            messages: []
          } as Room,
          ...existingRooms
        ];
      })
    );
    
    this.currentRoom$ = this.chatService.getCurrentRoom();
  }

  ngOnInit() {}

  joinRoom(room: Room) {
    this.chatService.setCurrentRoom(room);
  }

  getIcon(room: Room): string {
    if (room.isSystem) return 'forum';
    if (room.isPrivate) return 'lock';
    return 'chat';
  }
} 